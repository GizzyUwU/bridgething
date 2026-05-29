package dev.bridgething.spotify

import io.ktor.client.HttpClient
import io.ktor.client.engine.HttpClientEngine
import io.ktor.client.engine.cio.CIO
import io.ktor.client.request.delete
import io.ktor.client.request.get
import io.ktor.client.request.header
import io.ktor.client.request.post
import io.ktor.client.request.put
import io.ktor.client.request.setBody
import io.ktor.client.statement.HttpResponse
import io.ktor.client.statement.bodyAsText
import io.ktor.http.ContentType
import io.ktor.http.HttpMethod
import io.ktor.http.contentType
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.KSerializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.serializer

const val SPOTIFY_API_BASE = "https://api.spotify.com"

/** ktor-backed request layer against the spotify web api; owns token injection, 401-refresh-and-retry, and 429 backoff. */
class SpotinyHttp(private val client: SpotinyClient, engine: HttpClientEngine? = null) {

    val json = Json {
        ignoreUnknownKeys = true
        isLenient = true
        coerceInputValues = true
    }

    private val rateLimiter = RateLimiter()
    private val healthScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private var recoverJob: Job? = null

    // engine is injectable so tests can drive canned responses; prod uses CIO.
    private val http: HttpClient = engine?.let { HttpClient(it) } ?: HttpClient(CIO)

    suspend inline fun <reified T> getJson(url: String): T? =
        getJson(url, serializer<T>())

    suspend inline fun <reified T> getJson(path: String, query: Map<String, String>): T? =
        getJson(buildUrl(path, query), serializer<T>())

    suspend fun <T> getJson(url: String, deserializer: KSerializer<T>): T? {
        var i = 0
        while (i < client.maxRetries) {
            val response = send(url, HttpMethod.Get, null)
            if (response != null) {
                if (response.status == 204) return null
                val parsed = decodeWith(response.body, deserializer)
                if (parsed != null) return parsed
            }

            if (rateLimiter.isLimited()) return null
            if (client.needsReAuth) {
                client.reauthenticate()
                return null
            }
            i += 1
        }
        return null
    }

    suspend fun post(url: String, body: String? = null): Boolean = mutate(url, HttpMethod.Post, body)

    suspend fun put(url: String, body: String? = null): Boolean = mutate(url, HttpMethod.Put, body)

    suspend fun delete(url: String, body: String? = null): Boolean = mutate(url, HttpMethod.Delete, body)

    private suspend fun mutate(url: String, method: HttpMethod, body: String?): Boolean {
        var i = 0
        while (i < client.maxRetries) {
            val response = send(url, method, body)
            if (response != null && response.status in 200..299) return true

            if (rateLimiter.isLimited()) return false
            if (client.needsReAuth) {
                client.reauthenticate()
                return false
            }
            i += 1
        }
        return false
    }

    suspend fun <Response, Item> getMany(
        url: String,
        deserializer: KSerializer<Response>,
        extract: (Response) -> Triple<String?, Int, List<Item>>,
        limit: Int? = null,
        offset: Int? = null,
    ): ItemsPage<Item> {
        val values = mutableListOf<Item>()
        var total = 0

        val queryParams = mutableListOf<String>()
        if (limit != null && limit < 49) queryParams.add("limit=$limit")
        if (offset != null) queryParams.add("offset=$offset")

        var endpoint = url
        if (queryParams.isNotEmpty()) {
            endpoint += (if (url.contains("?")) "&" else "?") + queryParams.joinToString("&")
        }

        while (true) {
            val page = getJson(endpoint, deserializer) ?: return ItemsPage(values.toList(), total)
            val (next, pageTotal, items) = extract(page)
            values.addAll(items)
            total = pageTotal

            if (limit != null && values.size >= limit) break
            if (next != null) endpoint = next else break
        }

        return if (limit != null && limit > 0) {
            ItemsPage(values.take(limit), total)
        } else {
            ItemsPage(values.toList(), total)
        }
    }

    suspend inline fun <reified Item> getItems(
        url: String,
        limit: Int? = null,
        offset: Int? = null,
    ): ItemsPage<Item> = getMany(
        url = url,
        deserializer = serializer<ItemsResponse<Item>>(),
        extract = { Triple(it.next, it.total, it.items) },
        limit = limit,
        offset = offset,
    )

    suspend fun getFollowedArtists(
        url: String,
        limit: Int? = null,
        offset: Int? = null,
    ): ItemsPage<Artist> = getMany(
        url = url,
        deserializer = FollowedArtistsResponse.serializer(),
        extract = { Triple(it.artists.next, it.artists.total, it.artists.items) },
        limit = limit,
        offset = offset,
    )

    suspend fun getCategoryPlaylists(
        url: String,
        limit: Int? = null,
        offset: Int? = null,
    ): ItemsPage<Playlist> = getMany(
        url = url,
        deserializer = CategoryPlaylistsResponse.serializer(),
        extract = { Triple(it.playlists.next, it.playlists.total, it.playlists.items) },
        limit = limit,
        offset = offset,
    )

    private fun <T> decodeWith(text: String, deserializer: KSerializer<T>): T? {
        if (text.isEmpty()) return null
        return runCatching { json.decodeFromString(deserializer, text) }.getOrNull()
    }

    fun <T> decodeLossyArray(text: String, elementDeserializer: KSerializer<T>): List<T>? {
        if (text.isEmpty()) return null
        val root = runCatching { json.parseToJsonElement(text) }.getOrNull() as? JsonArray ?: return null
        return root.mapNotNull { element ->
            runCatching { json.decodeFromJsonElement(elementDeserializer, element) }.getOrNull()
        }
    }

    private suspend fun send(url: String, method: HttpMethod, body: String?): RawResponse? {
        val accessToken = client.accessToken
        if (accessToken.isEmpty()) {
            client.setNeedsReAuth(true)
            return null
        }
        if (client.needsReAuth) return null
        if (rateLimiter.isLimited()) return null

        val apply: io.ktor.client.request.HttpRequestBuilder.() -> Unit = {
            header("Authorization", "Bearer $accessToken")
            if (body != null) {
                contentType(ContentType.Application.Json)
                setBody(body)
            }
            if (url.contains("/me/player/queue")) {
                header("Accept-Encoding", "identity")
            }
        }

        val response: HttpResponse = try {
            when (method) {
                HttpMethod.Get -> http.get(url, apply)
                HttpMethod.Post -> http.post(url, apply)
                HttpMethod.Put -> http.put(url, apply)
                HttpMethod.Delete -> http.delete(url, apply)
                else -> http.get(url, apply)
            }
        } catch (e: Throwable) {
            return null
        }

        val statusCode = response.status.value

        if (statusCode == 401) {
            client.setNeedsReAuth(true)
            return null
        }

        if (statusCode == 429) {
            val seconds = response.headers["Retry-After"]?.toIntOrNull() ?: 60
            rateLimiter.markLimited(response.headers["Retry-After"])
            client.delegate?.serviceDidRateLimit(seconds)
            recoverJob?.cancel()
            recoverJob = healthScope.launch {
                delay(seconds * 1000L)
                client.delegate?.serviceDidRecover()
            }
            return null
        }

        if (statusCode !in 200..299) return null

        return RawResponse(statusCode, response.bodyAsText())
    }

    fun buildUrl(path: String, query: Map<String, String>): String {
        val base = if (path.startsWith("http")) path else SPOTIFY_API_BASE + path
        if (query.isEmpty()) return base
        val sep = if (base.contains("?")) "&" else "?"
        return base + sep + query.entries.joinToString("&") { (k, v) -> "$k=$v" }
    }

    private data class RawResponse(val status: Int, val body: String)
}

private class RateLimiter {
    private val mutex = Mutex()
    private var limitedUntil: Long? = null

    suspend fun isLimited(): Boolean = mutex.withLock {
        val until = limitedUntil ?: return false
        if (System.currentTimeMillis() < until) return true
        limitedUntil = null
        false
    }

    suspend fun markLimited(retryAfter: String?) = mutex.withLock {
        val seconds = retryAfter?.toIntOrNull() ?: 60
        limitedUntil = System.currentTimeMillis() + seconds * 1000L
    }
}

