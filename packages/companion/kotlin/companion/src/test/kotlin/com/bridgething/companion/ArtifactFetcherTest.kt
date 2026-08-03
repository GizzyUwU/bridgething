package com.bridgething.companion

import java.io.File
import java.io.IOException
import java.security.MessageDigest
import java.util.UUID
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Protocol
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.assertThrows

private class StubServer(private val code: Int = 200, private val payload: ByteArray) {
    val calls = AtomicInteger(0)

    fun client(): OkHttpClient = OkHttpClient.Builder()
        .addInterceptor { chain ->
            calls.incrementAndGet()
            Response.Builder()
                .request(chain.request())
                .protocol(Protocol.HTTP_1_1)
                .code(code)
                .message(if (code == 200) "OK" else "nope")
                .body(payload.toResponseBody("application/octet-stream".toMediaType()))
                .build()
        }
        .build()
}

private fun sha256(bytes: ByteArray): String =
    MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { String.format("%02x", it) }

private fun scratchDir(): File =
    File(System.getProperty("java.io.tmpdir"), "artifact-fetch-test-${UUID.randomUUID()}").also { it.mkdirs() }

class ArtifactFetcherTest {
    private val payload = ByteArray(4096) { (it % 251).toByte() }

    private fun fetcher(server: StubServer) = ArtifactFetcher(server.client(), Json { ignoreUnknownKeys = true })

    @Test
    fun `a verified artifact lands under its content address`() = runBlocking {
        val dir = scratchDir()
        val digest = ArtifactDigest(payload.size.toLong(), sha256(payload))

        val landed = fetcher(StubServer(payload = payload))
            .downloadIfNeeded("https://ota.example/x.bin", dir, "artifact", "test", digest)

        assertEquals("artifact-${digest.sha256}", landed.name)
        assertTrue(landed.readBytes().contentEquals(payload))
    }

    @Test
    fun `a sha mismatch refuses to land the artifact`() = runBlocking {
        val dir = scratchDir()
        val digest = ArtifactDigest(payload.size.toLong(), sha256(ByteArray(8)))

        assertThrows<IOException> {
            runBlocking {
                fetcher(StubServer(payload = payload))
                    .downloadIfNeeded("https://ota.example/x.bin", dir, "artifact", "test", digest)
            }
        }

        assertEquals(emptyList<String>(), dir.list()?.toList())
    }

    @Test
    fun `a size mismatch refuses to land the artifact`() = runBlocking {
        val dir = scratchDir()
        val digest = ArtifactDigest(payload.size.toLong() + 1, sha256(payload))

        assertThrows<IOException> {
            runBlocking {
                fetcher(StubServer(payload = payload))
                    .downloadIfNeeded("https://ota.example/x.bin", dir, "artifact", "test", digest)
            }
        }

        assertFalse(File(dir, "artifact-${digest.sha256}").exists())
    }

    @Test
    fun `a cached artifact is not fetched again`() = runBlocking {
        val dir = scratchDir()
        val digest = ArtifactDigest(payload.size.toLong(), sha256(payload))
        val server = StubServer(payload = payload)
        val fetcher = fetcher(server)

        fetcher.downloadIfNeeded("https://ota.example/x.bin", dir, "artifact", "test", digest)
        fetcher.downloadIfNeeded("https://ota.example/x.bin", dir, "artifact", "test", digest)

        assertEquals(1, server.calls.get())
    }

    @Test
    fun `an http error is surfaced`() = runBlocking {
        val dir = scratchDir()

        val error = assertThrows<IOException> {
            runBlocking {
                fetcher(StubServer(code = 404, payload = ByteArray(0)))
                    .downloadIfNeeded("https://ota.example/x.bin", dir, "artifact", "test", null)
            }
        }

        assertTrue(error.message!!.contains("404"), error.message)
    }

    @Test
    fun `progress reports the received byte counts`() = runBlocking {
        val dir = scratchDir()
        val digest = ArtifactDigest(payload.size.toLong(), sha256(payload))
        val ticks = mutableListOf<Long>()

        fetcher(StubServer(payload = payload))
            .downloadIfNeeded("https://ota.example/x.bin", dir, "artifact", "test", digest) { received, _ ->
                ticks.add(received)
            }

        assertEquals(payload.size.toLong(), ticks.last())
        assertTrue(ticks.zipWithNext().all { (a, b) -> b > a }, "progress must be monotonic: $ticks")
    }

    @Test
    fun `fetchJson decodes the response body`() = runBlocking {
        val body = """{"version":"1.0.0","updated_at":"2026-08-02T00:00:00Z"}""".toByteArray()

        val manifest = fetcher(StubServer(payload = body))
            .fetchJson(ModelManifest.serializer(), "https://ota.example/manifest.json")

        assertEquals("1.0.0", manifest.version)
    }

    @Test
    fun `hashing a file matches the digest of its bytes`() = runBlocking {
        val dir = scratchDir()
        val file = File(dir, "blob.bin").also { it.writeBytes(payload) }

        assertEquals(sha256(payload), hashFile(file))
    }
}
