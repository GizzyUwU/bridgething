package com.bridgething.companion

import com.bridgething.gateway.BridgethingGateway
import com.bridgething.gateway.device
import com.bridgething.gateway.transfer
import com.bridgething.schema.TransferAbandon
import com.bridgething.schema.TransferAck
import com.bridgething.schema.TransferFragment
import com.bridgething.schema.TransferRef
import java.io.IOException
import java.security.MessageDigest
import java.util.UUID
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

internal class TransferReceiver(private val gateway: BridgethingGateway) {
    private val mutex = Mutex()
    private val registrations = mutableMapOf<UUID, Registration>()
    private val pending = mutableMapOf<UUID, MutableList<PendingFragment>>()

    fun start(scope: CoroutineScope) {
        scope.launch { gateway.transfer.fragment.collect { (deviceId, frag) -> onFragment(deviceId, frag) } }
        scope.launch { gateway.transfer.abandon.collect { (_, ab) -> onAbandon(ab) } }
    }

    suspend fun receive(deviceId: String, ref: TransferRef): ByteArray {
        if (ref.totalSize.toLong() > MAX_TRANSFER_BYTES) {
            throw IOException("transfer ${ref.id}: totalSize ${ref.totalSize} exceeds $MAX_TRANSFER_BYTES cap")
        }
        val reg = Registration(deviceId, ref)
        val acks = mutableListOf<AckOut>()
        mutex.withLock {
            registrations[ref.id] = reg
            pending.remove(ref.id)?.sortedBy { it.fragment.offset.toLong() }?.forEach { queued ->
                if (ref.id !in registrations) return@forEach
                applyLocked(reg, queued.fragment)?.let { acks.add(it) }
            }
        }
        acks.forEach { sendAck(it) }
        return reg.result.await()
    }

    private suspend fun onFragment(deviceId: String, frag: TransferFragment) {
        val ack = mutex.withLock {
            val reg = registrations[frag.transferId]
            if (reg == null) {
                bufferPendingLocked(frag)
                null
            } else {
                applyLocked(reg, frag)
            }
        }
        ack?.let { sendAck(it) }
    }

    private suspend fun onAbandon(ab: TransferAbandon) {
        mutex.withLock {
            registrations.remove(ab.transferId)?.result?.completeExceptionally(
                IOException("transfer ${ab.transferId} abandoned by sender: ${ab.reason}"),
            )
            pending.remove(ab.transferId)
        }
    }

    private fun applyLocked(reg: Registration, frag: TransferFragment): AckOut? {
        if (frag.offset.toLong() != reg.received.toLong()) {
            fail(reg, IOException("transfer ${reg.ref.id}: gap at offset ${frag.offset}, expected ${reg.received}"))
            return null
        }
        if (reg.received.toLong() + frag.bytes.size > reg.ref.totalSize.toLong()) {
            fail(reg, IOException("transfer ${reg.ref.id}: fragment overruns totalSize ${reg.ref.totalSize}"))
            return null
        }
        frag.bytes.copyInto(reg.buffer, reg.received)
        reg.digest?.update(frag.bytes)
        reg.received += frag.bytes.size

        if (reg.received.toLong() == reg.ref.totalSize.toLong()) {
            val want = reg.ref.sha256
            if (want != null) {
                val got = reg.digest!!.digest().joinToString("") { "%02x".format(it) }
                if (got != want.lowercase()) {
                    fail(reg, IOException("transfer ${reg.ref.id}: sha256 $got != expected $want"))
                    return null
                }
            }
            registrations.remove(reg.ref.id)
            reg.result.complete(reg.buffer)
            return AckOut(reg.deviceId, reg.ref.id, reg.received.toUInt())
        }
        if (reg.received - reg.lastAcked >= ACK_COALESCE_BYTES) {
            reg.lastAcked = reg.received
            return AckOut(reg.deviceId, reg.ref.id, reg.received.toUInt())
        }
        return null
    }

    private fun fail(reg: Registration, e: Throwable) {
        registrations.remove(reg.ref.id)
        reg.result.completeExceptionally(e)
    }

    private fun bufferPendingLocked(frag: TransferFragment) {
        val now = System.currentTimeMillis()
        pending.values.forEach { list -> list.removeAll { now - it.atMs > PENDING_TTL_MS } }
        pending.entries.removeAll { it.value.isEmpty() }
        pending.getOrPut(frag.transferId) { mutableListOf() }.add(PendingFragment(frag, now))
        var total = pending.values.sumOf { it.size }
        while (total > MAX_PENDING_FRAGMENTS) {
            val oldest = pending.values.mapNotNull { it.firstOrNull() }.minByOrNull { it.atMs } ?: break
            pending.values.forEach { it.remove(oldest) }
            pending.entries.removeAll { it.value.isEmpty() }
            total--
        }
    }

    private suspend fun sendAck(ack: AckOut) {
        gateway.device(ack.deviceId).transfer.ack(TransferAck(transferId = ack.transferId, received = ack.received))
    }

    private class Registration(val deviceId: String, val ref: TransferRef) {
        val buffer = ByteArray(ref.totalSize.toInt())
        val digest = ref.sha256?.let { MessageDigest.getInstance("SHA-256") }
        val result = CompletableDeferred<ByteArray>()
        var received = 0
        var lastAcked = 0
    }

    private class PendingFragment(val fragment: TransferFragment, val atMs: Long)

    private data class AckOut(val deviceId: String, val transferId: UUID, val received: UInt)

    private companion object {
        const val ACK_COALESCE_BYTES = 16 * 1024
        const val MAX_TRANSFER_BYTES = 1 * 1024 * 1024L
        const val MAX_PENDING_FRAGMENTS = 64
        const val PENDING_TTL_MS = 5_000L
    }
}
