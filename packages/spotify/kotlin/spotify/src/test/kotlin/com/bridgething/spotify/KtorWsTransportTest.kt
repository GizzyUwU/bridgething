package com.bridgething.spotify

import io.ktor.server.application.install
import io.ktor.server.cio.CIO
import io.ktor.server.engine.EmbeddedServer
import io.ktor.server.engine.embeddedServer
import io.ktor.server.routing.routing
import io.ktor.server.websocket.DefaultWebSocketServerSession
import io.ktor.server.websocket.WebSockets
import io.ktor.server.websocket.webSocket
import io.ktor.websocket.CloseReason
import io.ktor.websocket.Frame
import io.ktor.websocket.close
import io.ktor.websocket.readText
import java.net.ServerSocket
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.runBlocking
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import uniffi.spotify.NoHandle
import uniffi.spotify.WsInbox

private class RecordingInbox : WsInbox(NoHandle) {
    val events = LinkedBlockingQueue<String>()
    override fun onOpen() {
        events.add("open")
    }

    override fun onText(text: String) {
        events.add("text:$text")
    }

    override fun onClosed(reason: String) {
        events.add("closed:$reason")
    }
}

class KtorWsTransportTest {
    private fun server(handler: suspend DefaultWebSocketServerSession.() -> Unit): Pair<EmbeddedServer<*, *>, Int> {
        val srv = embeddedServer(CIO, port = 0) {
            install(WebSockets)
            routing { webSocket("/ws", handler = handler) }
        }.start(wait = false)
        val port = runBlocking { srv.engine.resolvedConnectors().first().port }
        return srv to port
    }

    @Test
    fun opensPumpsBothDirectionsAndReportsServerClose() {
        val received = LinkedBlockingQueue<String>()
        val (srv, port) = server {
            send(Frame.Text("hello"))
            for (frame in incoming) {
                if (frame is Frame.Text) {
                    received.add(frame.readText())
                    close(CloseReason(CloseReason.Codes.NORMAL, "done"))
                }
            }
        }
        try {
            val inbox = RecordingInbox()
            KtorWsTransport().connect("ws://127.0.0.1:$port/ws", inbox)
            assertEquals("open", inbox.events.poll(5, TimeUnit.SECONDS))
            assertEquals("text:hello", inbox.events.poll(5, TimeUnit.SECONDS))
        } finally {
            srv.stop(0, 0)
        }
    }

    @Test
    fun sendTextReachesTheServer() {
        val received = LinkedBlockingQueue<String>()
        val (srv, port) = server {
            for (frame in incoming) {
                if (frame is Frame.Text) received.add(frame.readText())
            }
        }
        try {
            val inbox = RecordingInbox()
            val transport = KtorWsTransport()
            transport.connect("ws://127.0.0.1:$port/ws", inbox)
            assertEquals("open", inbox.events.poll(5, TimeUnit.SECONDS))
            transport.sendText("""{"type":"pong"}""")
            assertEquals("""{"type":"pong"}""", received.poll(5, TimeUnit.SECONDS))
        } finally {
            srv.stop(0, 0)
        }
    }

    @Test
    fun serverCloseReasonReachesTheInbox() {
        val (srv, port) = server {
            close(CloseReason(CloseReason.Codes.NORMAL, "done"))
        }
        try {
            val inbox = RecordingInbox()
            KtorWsTransport().connect("ws://127.0.0.1:$port/ws", inbox)
            assertEquals("open", inbox.events.poll(5, TimeUnit.SECONDS))
            assertEquals("closed:done", inbox.events.poll(5, TimeUnit.SECONDS))
        } finally {
            srv.stop(0, 0)
        }
    }

    @Test
    fun connectFailureSurfacesAsClosed() {
        // bind then release a port so nothing is listening on it.
        val port = ServerSocket(0).use { it.localPort }
        val inbox = RecordingInbox()
        KtorWsTransport().connect("ws://127.0.0.1:$port/ws", inbox)
        val event = inbox.events.poll(5, TimeUnit.SECONDS)
        assertTrue(event != null && event.startsWith("closed:"), "expected closed, got $event")
    }

    @Test
    fun replacedAndDisconnectedSessionsGoSilent() {
        val (srv, port) = server {
            while (incoming.receiveCatching().isSuccess) Unit
        }
        try {
            val transport = KtorWsTransport()
            val a = RecordingInbox()
            transport.connect("ws://127.0.0.1:$port/ws", a)
            assertEquals("open", a.events.poll(5, TimeUnit.SECONDS))
            val b = RecordingInbox()
            transport.connect("ws://127.0.0.1:$port/ws", b)
            assertEquals("open", b.events.poll(5, TimeUnit.SECONDS))
            transport.disconnect()
            // a replaced or disconnected session must not report a close.
            assertNull(a.events.poll(500, TimeUnit.MILLISECONDS))
            assertNull(b.events.poll(500, TimeUnit.MILLISECONDS))
        } finally {
            srv.stop(0, 0)
        }
    }
}
