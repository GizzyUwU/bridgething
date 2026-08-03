package com.bridgething.companion

import java.io.File
import java.io.IOException
import java.security.MessageDigest
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.DeserializationStrategy
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import okhttp3.OkHttpClient
import okhttp3.Request

@Serializable
public data class ArtifactDigest(
    val size: Long,
    val sha256: String,
)

internal class DigestMismatchException(asset: String, field: String) :
    IOException("$asset $field does not match the manifest; refusing to install")

internal class ArtifactFetcher(
    private val httpClient: OkHttpClient,
    private val json: Json,
) {
    suspend fun <T> fetchJson(deserializer: DeserializationStrategy<T>, url: String): T = withContext(Dispatchers.IO) {
        val req = Request.Builder().url(url).build()
        httpClient.newCall(req).execute().use { resp ->
            if (!resp.isSuccessful) throw IOException("manifest fetch returned HTTP ${resp.code}")
            val body = resp.body?.string() ?: throw IOException("manifest fetch returned empty body")
            json.decodeFromString(deserializer, body)
        }
    }

    suspend fun downloadIfNeeded(
        url: String,
        dir: File,
        filename: String,
        asset: String,
        expected: ArtifactDigest?,
        onProgress: (suspend (received: Long, total: Long) -> Unit)? = null,
    ): File = withContext(Dispatchers.IO) {
        if (!dir.exists()) dir.mkdirs()
        val cacheName = expected?.let { "$filename-${it.sha256}" } ?: filename
        val target = File(dir, cacheName)
        if (target.exists()) {
            val size = target.length()
            val reusable = if (expected != null) size == expected.size else size > 0L
            if (reusable) return@withContext target
            target.delete()
        }

        val tmp = File(dir, "$cacheName.download")
        try {
            val req = Request.Builder().url(url).build()
            httpClient.newCall(req).execute().use { resp ->
                if (!resp.isSuccessful) throw IOException("artifact fetch returned HTTP ${resp.code}")
                val body = resp.body ?: throw IOException("artifact fetch returned empty body")
                val total = body.contentLength().coerceAtLeast(0L)
                body.byteStream().use { input ->
                    tmp.outputStream().use { out ->
                        val buffer = ByteArray(64 * 1024)
                        var received = 0L
                        while (true) {
                            val read = input.read(buffer)
                            if (read == -1) break
                            out.write(buffer, 0, read)
                            received += read
                            onProgress?.invoke(received, total)
                        }
                    }
                }
            }
            if (expected != null) {
                if (tmp.length() != expected.size) throw DigestMismatchException(asset, "size")
                if (hashFile(tmp) != expected.sha256) throw DigestMismatchException(asset, "sha256")
            }
            if (target.exists()) target.delete()
            if (!tmp.renameTo(target)) throw IOException("failed to move downloaded artifact into cache")
            target
        } catch (e: Throwable) {
            tmp.delete()
            throw e
        }
    }
}

internal suspend fun hashFile(file: File): String = withContext(Dispatchers.IO) {
    val md = MessageDigest.getInstance("SHA-256")
    file.inputStream().use { input ->
        val buf = ByteArray(64 * 1024)
        while (true) {
            val n = input.read(buf)
            if (n <= 0) break
            md.update(buf, 0, n)
        }
    }
    md.digest().joinToString("") { String.format("%02x", it) }
}
