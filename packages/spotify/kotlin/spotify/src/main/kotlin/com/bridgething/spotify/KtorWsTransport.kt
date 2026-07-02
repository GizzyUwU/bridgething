package com.bridgething.spotify

import io.ktor.client.HttpClient
import io.ktor.client.engine.cio.CIO
import io.ktor.client.plugins.websocket.WebSockets
import io.ktor.client.plugins.websocket.webSocket
import io.ktor.websocket.Frame
import io.ktor.websocket.readText
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import uniffi.spotify.WsInbox
import uniffi.spotify.WsTransport

/**
 * Android [WsTransport] for the Spotify dealer, the Kotlin counterpart to
 * iOS's `UrlSessionWsTransport`. The crate's native tungstenite default does
 * not work inside the uniffi async runtime on Android (the spawned task is
 * dropped and the socket never opens). [connect] must not block; a replaced
 * or [disconnect]ed session goes silent instead of reporting a close.
 */
class KtorWsTransport : WsTransport {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    // dedicated client: the dealer socket is long-lived, so no HttpTimeout plugin
    // and no engine request timeout - either would sever an established session.
    private val client: HttpClient by lazy {
        HttpClient(CIO) {
            install(WebSockets)
            engine {
                requestTimeout = 0
            }
        }
    }

    private class Conn(val outbound: Channel<String>, val job: Job) {
        fun shutdown() {
            job.cancel()
            outbound.close()
        }
    }

    private val conn = AtomicReference<Conn?>(null)

    override fun connect(url: String, inbox: WsInbox) {
        val outbound = Channel<String>(Channel.UNLIMITED)
        val job = scope.launch { pump(url, inbox, outbound) }
        conn.getAndSet(Conn(outbound, job))?.shutdown()
    }

    override fun sendText(text: String) {
        conn.get()?.outbound?.trySend(text)
    }

    override fun disconnect() {
        conn.getAndSet(null)?.shutdown()
    }

    private suspend fun pump(url: String, inbox: WsInbox, outbound: Channel<String>) {
        try {
            var reason = "closed"
            client.webSocket(url) {
                inbox.onOpen()
                val writer = launch {
                    for (text in outbound) send(Frame.Text(text))
                }
                try {
                    // dealer is json over text frames; binary is ignored and
                    // protocol ping/pong is handled by the default session.
                    for (frame in incoming) {
                        if (frame is Frame.Text) inbox.onText(frame.readText())
                    }
                    reason = closeReason.await()?.message?.takeIf { it.isNotEmpty() } ?: "closed"
                } finally {
                    writer.cancel()
                }
            }
            inbox.onClosed(reason)
        } catch (c: CancellationException) {
            throw c
        } catch (t: Throwable) {
            inbox.onClosed(t.message ?: t.toString())
        } finally {
            runCatching { inbox.close() }
        }
    }
}
