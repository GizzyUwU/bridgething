package com.bridgething.companion

import com.bridgething.gateway.BridgethingGateway
import com.bridgething.schema.BridgeToGatewayMsgData
import com.bridgething.schema.BridgeToGatewayTransferMsg
import com.bridgething.schema.BridgeToGatewayWebappMsg
import com.bridgething.schema.GatewayToBridgeMsg
import com.bridgething.schema.GatewayToBridgeMsgData
import com.bridgething.schema.GatewayToBridgeWebappMsg
import com.bridgething.schema.MsgMeta
import com.bridgething.schema.ResponseMeta
import com.bridgething.schema.TransferBody
import com.bridgething.schema.TransferFragment
import com.bridgething.schema.TransferRef
import com.bridgething.schema.WebappResource
import com.bridgething.schema.WebappResourceKind
import com.bridgething.schema.WebappResourceReply
import java.io.File
import java.security.MessageDigest
import java.util.UUID
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.runBlocking
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import kotlin.time.Duration.Companion.seconds

class WebappResourceServiceTest {
    private data class Harness(
        val gateway: BridgethingGateway,
        val driver: WireDriver,
        val receiver: TransferReceiver,
        val bg: CoroutineScope,
    )

    private suspend fun boot(): Harness {
        val adapter = FakeAdapter()
        val gateway = BridgethingGateway(adapter)
        gateway.start()
        val bg = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val driver = WireDriver(adapter)
        driver.start(bg)
        driver.connect()
        val receiver = TransferReceiver(gateway)
        receiver.start(bg)
        return Harness(gateway, driver, receiver, bg)
    }

    private fun sha256(bytes: ByteArray): String =
        MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }

    private fun asResourceRequest(msg: GatewayToBridgeMsg): WebappResource? =
        ((msg.data as? GatewayToBridgeMsgData.Webapp)?.data as? GatewayToBridgeWebappMsg.Resource)?.data

    private suspend fun WireDriver.reply(requestId: UUID, reply: WebappResourceReply) = send(
        BridgeToGatewayMsgData.Webapp(BridgeToGatewayWebappMsg.Resource(reply)),
        meta = MsgMeta.Response(ResponseMeta(requestId = requestId)),
    )

    private suspend fun WireDriver.awaitResourceRequest(): Pair<UUID, WebappResource> {
        val frame = waitOutbound(2.seconds) { asResourceRequest(it) != null }
        return frame.id to asResourceRequest(frame)!!
    }

    @Test
    fun `inline fetch caches and a matching have serves the cache bodyless`() = runBlocking {
        val (gateway, driver, receiver, bg) = boot()
        val tmp = File(System.getProperty("java.io.tmpdir"), "btres-${UUID.randomUUID()}")
        val service = WebappResourceService(cacheDir = tmp, gateway = gateway, receiver = receiver)
        val webappId = UUID.randomUUID()
        val bytes = ByteArray(2048) { (it % 251).toByte() }
        val sha = sha256(bytes)

        val first = async { service.fetch(driver.deviceId, webappId, WebappResourceKind.Icon) }
        val (id1, req1) = driver.awaitResourceRequest()
        assertNull(req1.have, "first fetch has nothing cached, so no have")
        driver.reply(id1, WebappResourceReply(id = webappId, kind = WebappResourceKind.Icon, sha256 = sha, mime = "image/png", body = TransferBody.Inline(bytes)))
        val hit1 = first.await()!!
        assertTrue(hit1.file.readBytes().contentEquals(bytes), "inline body must be written to the cache file")
        assertEquals(sha, hit1.sha256)
        assertEquals("image/png", hit1.mime)
        assertTrue(hit1.file.name.endsWith(".png"), "extension must derive from mime; got ${hit1.file.name}")

        val second = async { service.fetch(driver.deviceId, webappId, WebappResourceKind.Icon) }
        val (id2, req2) = driver.awaitResourceRequest()
        assertEquals(sha, req2.have, "second fetch must offer the cached sha")
        driver.reply(id2, WebappResourceReply(id = webappId, kind = WebappResourceKind.Icon, sha256 = sha, mime = "image/png", body = null))
        val hit2 = second.await()!!
        assertEquals(hit1.file, hit2.file, "a bodyless reply must serve the existing cache file")
        assertTrue(hit2.file.readBytes().contentEquals(bytes))

        tmp.deleteRecursively()
        bg.cancel()
        gateway.stop()
    }

    @Test
    fun `streamed fetch reassembles and caches`() = runBlocking {
        val (gateway, driver, receiver, bg) = boot()
        val tmp = File(System.getProperty("java.io.tmpdir"), "btres-${UUID.randomUUID()}")
        val service = WebappResourceService(cacheDir = tmp, gateway = gateway, receiver = receiver)
        val webappId = UUID.randomUUID()
        val size = 24 * 1024
        val chunk = 4 * 1024
        val bytes = ByteArray(size) { (it * 3 % 251).toByte() }
        val sha = sha256(bytes)
        val transferId = UUID.randomUUID()

        val job = async { service.fetch(driver.deviceId, webappId, WebappResourceKind.Settings) }
        val (reqId, req) = driver.awaitResourceRequest()
        assertEquals(WebappResourceKind.Settings, req.kind)
        driver.reply(
            reqId,
            WebappResourceReply(
                id = webappId, kind = WebappResourceKind.Settings, sha256 = sha, mime = "text/html",
                body = TransferBody.Stream(TransferRef(id = transferId, totalSize = size.toUInt(), sha256 = sha)),
            ),
        )

        var offset = 0
        while (offset < size) {
            val end = minOf(offset + chunk, size)
            driver.send(
                BridgeToGatewayMsgData.Transfer(
                    BridgeToGatewayTransferMsg.Fragment(TransferFragment(transferId = transferId, offset = offset.toUInt(), bytes = bytes.copyOfRange(offset, end))),
                ),
                meta = MsgMeta.Event,
            )
            offset = end
        }

        val hit = job.await()!!
        assertTrue(hit.file.readBytes().contentEquals(bytes), "streamed body must be reassembled into the cache file")
        assertEquals(sha, hit.sha256)
        assertTrue(hit.file.name.endsWith(".html"), "extension must derive from mime; got ${hit.file.name}")

        tmp.deleteRecursively()
        bg.cancel()
        gateway.stop()
    }
}
