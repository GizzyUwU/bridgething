package com.bridgething.companion

import com.bridgething.schema.BridgeToGatewayMsgData
import com.bridgething.schema.BridgeToGatewayTunnelMsg
import com.bridgething.schema.GatewayToBridgeMsgData
import com.bridgething.schema.GatewayToBridgeTunnelMsg
import com.bridgething.schema.TunnelClosed
import com.bridgething.schema.TunnelData
import com.bridgething.schema.TunnelOpen
import io.mockk.mockk
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.jupiter.api.Assertions.assertArrayEquals
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import java.net.ServerSocket
import java.util.UUID
import kotlin.concurrent.thread
import kotlin.time.Duration.Companion.seconds

class TunnelDispatchTest {
    private suspend fun boot(scope: CoroutineScope): Pair<BridgethingCompanion, WireDriver> {
        val adapter = FakeAdapter()
        val companion = BridgethingCompanion(
            context = mockk(relaxed = true),
            adapter = adapter,
            lyricsResolver = FakeLyricsResolver(),
            host = HostInfo(appName = "tunnel-test", appVersion = "0.0.1", osName = "test"),
            geo = NoOpGeoSource,
            volume = NoOpVolumeSource,
            audio = NoOpAudioBackend,
        )
        companion.start()
        val driver = WireDriver(adapter)
        driver.start(scope)
        driver.connect()
        return companion to driver
    }

    private class EchoServer : AutoCloseable {
        private val server = ServerSocket(0)
        val port: Int get() = server.localPort

        init {
            thread(isDaemon = true) {
                runCatching {
                    val conn = server.accept()
                    val input = conn.getInputStream()
                    val output = conn.getOutputStream()
                    val buf = ByteArray(64 * 1024)
                    while (true) {
                        val n = input.read(buf)
                        if (n < 0) break
                        output.write(buf, 0, n)
                        output.flush()
                    }
                    conn.close()
                }
            }
        }

        override fun close() {
            runCatching { server.close() }
        }
    }

    @Test
    fun `tunnel open echo close`() = runBlocking {
        EchoServer().use { echo ->
            val (companion, driver) = boot(this)
            val id = UUID.randomUUID()

            val openResp = withTimeout(5.seconds) {
                driver.request(
                    BridgeToGatewayMsgData.Tunnel(
                        BridgeToGatewayTunnelMsg.Open(TunnelOpen(tunnelId = id, host = "127.0.0.1", port = echo.port.toUShort())),
                    ),
                    5.seconds,
                )
            }
            val openOuter = openResp.data as GatewayToBridgeMsgData.Tunnel
            assertTrue(openOuter.data is GatewayToBridgeTunnelMsg.OpenReply, "expected OpenReply, got ${openOuter.data}")

            val payload = "ping-through-the-phone".toByteArray()
            driver.send(BridgeToGatewayMsgData.Tunnel(BridgeToGatewayTunnelMsg.Data(TunnelData(tunnelId = id, bytes = payload))))

            val echoed = withTimeout(5.seconds) {
                driver.waitOutbound { msg ->
                    (msg.data as? GatewayToBridgeMsgData.Tunnel)?.data is GatewayToBridgeTunnelMsg.Data
                }
            }
            val td = ((echoed.data as GatewayToBridgeMsgData.Tunnel).data as GatewayToBridgeTunnelMsg.Data).data
            assertEquals(id, td.tunnelId)
            assertArrayEquals(payload, td.bytes)

            driver.send(BridgeToGatewayMsgData.Tunnel(BridgeToGatewayTunnelMsg.Close(TunnelClosed(tunnelId = id, reason = null))))
            companion.stop()
        }
    }

    @Test
    fun `short writes are still acked so the window recovers`() = runBlocking {
        EchoServer().use { echo ->
            val (companion, driver) = boot(this)
            val id = UUID.randomUUID()

            val openResp = withTimeout(5.seconds) {
                driver.request(
                    BridgeToGatewayMsgData.Tunnel(
                        BridgeToGatewayTunnelMsg.Open(TunnelOpen(tunnelId = id, host = "127.0.0.1", port = echo.port.toUShort())),
                    ),
                    5.seconds,
                )
            }
            val openOuter = openResp.data as GatewayToBridgeMsgData.Tunnel
            assertTrue(openOuter.data is GatewayToBridgeTunnelMsg.OpenReply, "expected OpenReply, got ${openOuter.data}")

            val payload = ByteArray(1024) { 0x7a }
            driver.send(BridgeToGatewayMsgData.Tunnel(BridgeToGatewayTunnelMsg.Data(TunnelData(tunnelId = id, bytes = payload))))

            val acked = withTimeout(5.seconds) {
                driver.waitOutbound { msg ->
                    (msg.data as? GatewayToBridgeMsgData.Tunnel)?.data is GatewayToBridgeTunnelMsg.Ack
                }
            }
            val ack = ((acked.data as GatewayToBridgeMsgData.Tunnel).data as GatewayToBridgeTunnelMsg.Ack).data
            assertEquals(id, ack.tunnelId)
            assertEquals(payload.size.toUInt(), ack.consumed)

            driver.send(BridgeToGatewayMsgData.Tunnel(BridgeToGatewayTunnelMsg.Close(TunnelClosed(tunnelId = id, reason = null))))
            companion.stop()
        }
    }

    @Test
    fun `tunnel open to closed port returns error`() = runBlocking {
        val (companion, driver) = boot(this)
        val resp = withTimeout(5.seconds) {
            driver.request(
                BridgeToGatewayMsgData.Tunnel(
                    BridgeToGatewayTunnelMsg.Open(TunnelOpen(tunnelId = UUID.randomUUID(), host = "127.0.0.1", port = 1u)),
                ),
                5.seconds,
            )
        }
        val outer = resp.data as GatewayToBridgeMsgData.Tunnel
        assertTrue(outer.data is GatewayToBridgeTunnelMsg.ErrorReply, "expected ErrorReply, got ${outer.data}")
        companion.stop()
    }
}
