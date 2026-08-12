package com.bridgething.companion.shell

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
import uniffi.bridgething_companion.NoHandle
import uniffi.bridgething_companion.WsConnect
import uniffi.bridgething_companion.WsFrame
import uniffi.bridgething_companion.WsInbox

private class RecordingWsInbox : WsInbox(NoHandle) {
    val events = LinkedBlockingQueue<String>()

    override fun onOpen(id: String, acceptedProtocol: String?) {
        events.add("open:$id")
    }

    override fun onText(id: String, text: String) {
        events.add("text:$id:$text")
    }

    override fun onBinary(id: String, bytes: ByteArray) {
        events.add("binary:$id:${bytes.joinToString(",")}")
    }

    override fun onClosed(id: String, code: UShort?, reason: String) {
        events.add("closed:$id:${code ?: "?"}:$reason")
    }
}

class KtorWsTransportTest {
    private fun connect(id: String, port: Int) =
        WsConnect(id = id, url = "ws://127.0.0.1:$port/ws", protocols = emptyList(), headers = emptyList())

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
            val inbox = RecordingWsInbox()
            val transport = KtorWsTransport()
            transport.connect(connect("a", port), inbox)
            assertEquals("open:a", inbox.events.poll(5, TimeUnit.SECONDS))
            assertEquals("text:a:hello", inbox.events.poll(5, TimeUnit.SECONDS))
            transport.send("a", WsFrame.Text("""{"type":"pong"}"""))
            assertEquals("""{"type":"pong"}""", received.poll(5, TimeUnit.SECONDS))
            assertEquals("closed:a:1000:done", inbox.events.poll(5, TimeUnit.SECONDS))
        } finally {
            srv.stop(0, 0)
        }
    }

    @Test
    fun twoConnectionsAreIndependent() {
        val (srv, port) = server {
            for (frame in incoming) {
                if (frame is Frame.Text) send(Frame.Text("echo:${frame.readText()}"))
            }
        }
        try {
            val transport = KtorWsTransport()
            val a = RecordingWsInbox()
            val b = RecordingWsInbox()
            transport.connect(connect("a", port), a)
            transport.connect(connect("b", port), b)
            assertEquals("open:a", a.events.poll(5, TimeUnit.SECONDS))
            assertEquals("open:b", b.events.poll(5, TimeUnit.SECONDS))
            transport.send("a", WsFrame.Text("one"))
            transport.send("b", WsFrame.Text("two"))
            assertEquals("text:a:echo:one", a.events.poll(5, TimeUnit.SECONDS))
            assertEquals("text:b:echo:two", b.events.poll(5, TimeUnit.SECONDS))
        } finally {
            srv.stop(0, 0)
        }
    }

    @Test
    fun disconnectSendsTheCloseAndReportsIt() {
        val (srv, port) = server {
            while (incoming.receiveCatching().isSuccess) Unit
        }
        try {
            val transport = KtorWsTransport()
            val inbox = RecordingWsInbox()
            transport.connect(connect("a", port), inbox)
            assertEquals("open:a", inbox.events.poll(5, TimeUnit.SECONDS))
            transport.disconnect("a", 1001u, "going away")
            assertEquals("closed:a:1001:going away", inbox.events.poll(5, TimeUnit.SECONDS))
        } finally {
            srv.stop(0, 0)
        }
    }

    @Test
    fun connectFailureSurfacesAsClosed() {
        val port = ServerSocket(0).use { it.localPort }
        val inbox = RecordingWsInbox()
        KtorWsTransport().connect(connect("a", port), inbox)
        val event = inbox.events.poll(10, TimeUnit.SECONDS)
        assertTrue(event != null && event.startsWith("closed:a:"), "expected closed, got $event")
    }

    @Test
    fun aReplacedSessionGoesSilent() {
        val (srv, port) = server {
            while (incoming.receiveCatching().isSuccess) Unit
        }
        try {
            val transport = KtorWsTransport()
            val first = RecordingWsInbox()
            transport.connect(connect("a", port), first)
            assertEquals("open:a", first.events.poll(5, TimeUnit.SECONDS))
            val second = RecordingWsInbox()
            transport.connect(connect("a", port), second)
            assertEquals("open:a", second.events.poll(5, TimeUnit.SECONDS))
            assertNull(first.events.poll(500, TimeUnit.MILLISECONDS))
        } finally {
            srv.stop(0, 0)
        }
    }

    @Test
    fun binaryFramesCrossIntact() {
        val (srv, port) = server {
            send(Frame.Binary(true, byteArrayOf(1, 2, 3)))
            while (incoming.receiveCatching().isSuccess) Unit
        }
        try {
            val inbox = RecordingWsInbox()
            KtorWsTransport().connect(connect("a", port), inbox)
            assertEquals("open:a", inbox.events.poll(5, TimeUnit.SECONDS))
            assertEquals("binary:a:1,2,3", inbox.events.poll(5, TimeUnit.SECONDS))
        } finally {
            srv.stop(0, 0)
        }
    }
}
