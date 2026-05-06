package dev.bridgething.lyrics

import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.engine.cio.CIO
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.client.request.get
import io.ktor.client.request.header
import io.ktor.client.request.parameter
import io.ktor.client.statement.HttpResponse
import io.ktor.http.HttpStatusCode
import io.ktor.serialization.kotlinx.json.json
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

/**
 * `LyricsResolver` backed by lrclib.net's signature lookup endpoint.
 * No auth, free, community-uploaded LRC. Coverage is strong on western
 * mainstream and weak on remixes / live / regional / very new releases.
 */
class LrclibResolver(
    private val baseUrl: String = "https://lrclib.net",
    private val userAgent: String = "bridgething/0.1 (+https://github.com/thinglabsoss/bridgething)",
    httpClient: HttpClient? = null,
) : LyricsResolver {
    override val name: String = "lrclib"

    private val client: HttpClient = httpClient ?: HttpClient(CIO) {
        install(ContentNegotiation) {
            json(Json { ignoreUnknownKeys = true })
        }
    }

    override suspend fun lyrics(track: TrackIdentity): Lyrics? {
        val response: HttpResponse = try {
            client.get("$baseUrl/api/get") {
                header("User-Agent", userAgent)
                header("Accept", "application/json")
                parameter("artist_name", track.artist)
                parameter("track_name", track.track)
                track.album?.let { parameter("album_name", it) }
                track.durationMs?.let { parameter("duration", (it / 1000).toString()) }
            }
        } catch (e: Exception) {
            return null
        }

        if (response.status != HttpStatusCode.OK) return null

        val entry: LrclibEntry = try {
            response.body()
        } catch (e: Exception) {
            return null
        }

        return entry.toLyrics()
    }
}

@Serializable
private data class LrclibEntry(
    val plainLyrics: String? = null,
    val syncedLyrics: String? = null,
    val instrumental: Boolean? = null,
) {
    fun toLyrics(): Lyrics? {
        if (instrumental == true) {
            return Lyrics(synced = null, plain = null, source = "lrclib")
        }
        val synced = syncedLyrics?.let { LRCParser.parse(it) }?.takeIf { it.isNotEmpty() }
        val plain = plainLyrics?.takeIf { it.isNotEmpty() }
        if (synced == null && plain == null) return null
        return Lyrics(synced = synced, plain = plain, source = "lrclib")
    }
}

/**
 * Parses the LRC time-tagged format. Each line may carry one or more
 * `[mm:ss.xx]` timestamps followed by the line text; lines without any
 * timestamp are ignored. Multiple timestamps on a single line emit
 * multiple `LyricLine` entries with the same text.
 */
object LRCParser {
    fun parse(text: String): List<LyricLine> {
        val out = mutableListOf<LyricLine>()
        for (rawLine in text.split('\n')) {
            val (timestamps, body) = extractTimestamps(rawLine)
            if (timestamps.isEmpty()) continue
            for (ms in timestamps) {
                out += LyricLine(startMs = ms, text = body)
            }
        }
        return out.sortedBy { it.startMs }
    }

    private fun extractTimestamps(line: String): Pair<List<Int>, String> {
        val stamps = mutableListOf<Int>()
        var rest = line
        while (rest.startsWith("[")) {
            val close = rest.indexOf(']')
            if (close < 0) break
            val inside = rest.substring(1, close)
            val ms = parseTimestamp(inside) ?: break
            stamps += ms
            rest = rest.substring(close + 1)
        }
        return stamps to rest.trim()
    }

    private fun parseTimestamp(s: String): Int? {
        val parts = s.split(':')
        if (parts.size != 2) return null
        val minutes = parts[0].toIntOrNull() ?: return null
        val secondParts = parts[1].split('.')
        val seconds = secondParts[0].toIntOrNull() ?: return null
        var hundredths = 0
        if (secondParts.size == 2) {
            val frac = secondParts[1]
            val normalized = when (frac.length) {
                2 -> frac
                3 -> frac.substring(0, 2)
                else -> frac.padEnd(2, '0').substring(0, 2)
            }
            hundredths = normalized.toIntOrNull() ?: 0
        }
        return (minutes * 60 + seconds) * 1000 + hundredths * 10
    }
}
