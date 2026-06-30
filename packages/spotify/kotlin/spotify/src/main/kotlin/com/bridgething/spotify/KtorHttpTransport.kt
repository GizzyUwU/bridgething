package com.bridgething.spotify

import io.ktor.client.HttpClient
import io.ktor.client.engine.cio.CIO
import io.ktor.client.plugins.HttpTimeout
import io.ktor.client.plugins.timeout
import io.ktor.client.request.header
import io.ktor.client.request.request
import io.ktor.client.request.setBody
import io.ktor.client.statement.bodyAsBytes
import io.ktor.http.HttpMethod as KtorMethod
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import uniffi.spotify.HttpHeader
import uniffi.spotify.HttpMethod
import uniffi.spotify.HttpRequest
import uniffi.spotify.HttpResponse
import uniffi.spotify.HttpSink
import uniffi.spotify.HttpTransport

/**
 * Android [HttpTransport] for the Spotify worker. The Rust crate ships a native
 * [reqwest]-backed default, but reqwest does not work inside the uniffi async
 * runtime on Android (the spawned task is dropped and the [HttpSink] never gets
 * a response -> "http transport dropped without responding"). Each platform is
 * meant to install its own transport instead; this is the Kotlin counterpart to
 * iOS's `UrlSessionHttpTransport`, built on ktor's pure-Kotlin CIO engine.
 *
 * Contract: [execute] must not block - it hands the request to a coroutine and
 * returns. Exactly one of [HttpSink.complete] / [HttpSink.fail] must be called
 * for every request, including on failure, or the Rust side hangs until it
 * times out.
 */
class KtorHttpTransport : HttpTransport {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    private val client: HttpClient by lazy {
        HttpClient(CIO) {
            expectSuccess = false
            install(HttpTimeout) {
                // Generous ceilings; the Rust caller sets a per-request timeout
                // we honor below, and the device-code poll wants a long window.
                requestTimeoutMillis = 60_000
                connectTimeoutMillis = 15_000
            }
        }
    }

    override fun execute(request: HttpRequest, sink: HttpSink) {
        scope.launch {
            try {
                val resp = client.request(request.url) {
                    method = when (request.method) {
                        HttpMethod.GET -> KtorMethod.Get
                        HttpMethod.POST -> KtorMethod.Post
                        HttpMethod.PUT -> KtorMethod.Put
                    }
                    for (h in request.headers) header(h.name, h.value)
                    if (request.timeoutMs > 0u) {
                        timeout { requestTimeoutMillis = request.timeoutMs.toLong() }
                    }
                    if (request.body.isNotEmpty()) setBody(request.body)
                }
                val body = resp.bodyAsBytes()
                val headers = buildList {
                    resp.headers.forEach { name, values ->
                        for (v in values) add(HttpHeader(name, v))
                    }
                }
                sink.complete(
                    HttpResponse(
                        status = resp.status.value.toUShort(),
                        headers = headers,
                        body = body,
                    ),
                )
            } catch (t: Throwable) {
                runCatching { sink.fail(t.message ?: t.toString()) }
            } finally {
                runCatching { sink.close() }
            }
        }
    }
}
