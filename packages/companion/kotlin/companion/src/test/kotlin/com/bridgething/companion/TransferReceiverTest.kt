package com.bridgething.companion

import com.bridgething.gateway.BridgethingGateway
import com.bridgething.schema.BridgeToGatewayMsgData
import com.bridgething.schema.BridgeToGatewayTransferMsg
import com.bridgething.schema.GatewayToBridgeMsg
import com.bridgething.schema.GatewayToBridgeMsgData
import com.bridgething.schema.GatewayToBridgeTransferMsg
import com.bridgething.schema.MsgMeta
import com.bridgething.schema.TransferAbandon
import com.bridgething.schema.TransferFragment
import com.bridgething.schema.TransferRef
import java.io.IOException
import java.security.MessageDigest
import java.util.UUID
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import kotlin.time.Duration.Companion.seconds

class TransferReceiverTest {
    private data class Harness(val gateway: BridgethingGateway, val driver: WireDriver, val bg: CoroutineScope)

    private suspend fun boot(): Harness {
        val adapter = FakeAdapter()
        val gateway = BridgethingGateway(adapter)
        gateway.start()
        val bg = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val driver = WireDriver(adapter)
        driver.start(bg)
        driver.connect()
        return Harness(gateway, driver, bg)
    }

    private fun sha256(bytes: ByteArray): String =
        MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }

    private suspend fun WireDriver.sendFragment(transferId: UUID, offset: Int, bytes: ByteArray) = send(
        BridgeToGatewayMsgData.Transfer(
            BridgeToGatewayTransferMsg.Fragment(TransferFragment(transferId = transferId, offset = offset.toUInt(), bytes = bytes)),
        ),
        meta = MsgMeta.Event,
    )

    private fun ackReceived(msg: GatewayToBridgeMsg): Int =
        ((msg.data as GatewayToBridgeMsgData.Transfer).data as GatewayToBridgeTransferMsg.Ack).data.received.toInt()

    private fun isAck(msg: GatewayToBridgeMsg): Boolean =
        (msg.data as? GatewayToBridgeMsgData.Transfer)?.data is GatewayToBridgeTransferMsg.Ack

    @Test
    fun `streamed body reassembles and acks coalesce`() = runBlocking {
        val (gateway, driver, bg) = boot()
        val receiver = TransferReceiver(gateway)
        receiver.start(bg)

        val size = 40 * 1024
        val chunk = 4 * 1024
        val payload = ByteArray(size) { (it % 251).toByte() }
        val transferId = UUID.randomUUID()
        val ref = TransferRef(id = transferId, totalSize = size.toUInt(), sha256 = sha256(payload))

        val job = async { receiver.receive(driver.deviceId, ref) }
        var offset = 0
        while (offset < size) {
            val end = minOf(offset + chunk, size)
            driver.sendFragment(transferId, offset, payload.copyOfRange(offset, end))
            offset = end
        }

        val bytes = job.await()
        assertTrue(bytes.contentEquals(payload), "reassembled body must equal the sent payload")

        // acks coalesce to one per 16 KiB plus the always-sent final byte; never one per 4 KiB fragment.
        val acks = mutableListOf<Int>()
        while (acks.lastOrNull() != size) {
            acks.add(ackReceived(driver.waitOutbound(2.seconds) { isAck(it) }))
        }
        assertEquals(listOf(16 * 1024, 32 * 1024, size), acks, "ack cadence must coalesce and end on the final byte")

        bg.cancel()
        gateway.stop()
    }

    @Test
    fun `fragments that beat registration are drained from pending`() = runBlocking {
        val (gateway, driver, bg) = boot()
        val receiver = TransferReceiver(gateway)
        receiver.start(bg)

        val size = 16 * 1024
        val chunk = 4 * 1024
        val payload = ByteArray(size) { (it * 7 % 251).toByte() }
        val transferId = UUID.randomUUID()
        val ref = TransferRef(id = transferId, totalSize = size.toUInt(), sha256 = sha256(payload))

        // fragments arrive before anyone registers; the pending buffer must hold the head.
        var offset = 0
        while (offset < size) {
            val end = minOf(offset + chunk, size)
            driver.sendFragment(transferId, offset, payload.copyOfRange(offset, end))
            offset = end
        }
        delay(150)

        val bytes = receiver.receive(driver.deviceId, ref)
        assertTrue(bytes.contentEquals(payload), "a stream buffered before registration must still reassemble")
        assertEquals(size, ackReceived(driver.waitOutbound(2.seconds) { isAck(it) }), "final byte must be acked")

        bg.cancel()
        gateway.stop()
    }

    @Test
    fun `abandon fails an in-flight collect`() = runBlocking {
        val (gateway, driver, bg) = boot()
        val receiver = TransferReceiver(gateway)
        receiver.start(bg)

        val transferId = UUID.randomUUID()
        val ref = TransferRef(id = transferId, totalSize = (32 * 1024).toUInt(), sha256 = null)

        // run under the supervisor scope so the expected failure stays in the deferred instead of failing the test.
        val job = bg.async { receiver.receive(driver.deviceId, ref) }
        delay(100)
        driver.sendFragment(transferId, 0, ByteArray(4 * 1024))
        driver.send(
            BridgeToGatewayMsgData.Transfer(
                BridgeToGatewayTransferMsg.Abandon(TransferAbandon(transferId = transferId, reason = "upstream gone")),
            ),
            meta = MsgMeta.Event,
        )

        val failure = runCatching { job.await() }.exceptionOrNull()
        assertTrue(failure is IOException, "abandon must fail the collect")
        assertTrue(failure!!.message!!.contains("abandoned"), "failure must name the abandon; got ${failure.message}")

        bg.cancel()
        gateway.stop()
    }
}
