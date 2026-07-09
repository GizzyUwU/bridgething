package com.bridgething.companion

import java.io.IOException
import java.util.UUID
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull

internal class TransferAckWindow {
    private val mutex = Mutex()
    private val received = mutableMapOf<UUID, UInt>()
    private val version = MutableStateFlow(0L)

    suspend fun note(transferId: UUID, receivedBytes: UInt) {
        mutex.withLock {
            val prior = received[transferId] ?: 0u
            if (receivedBytes <= prior) return
            received[transferId] = receivedBytes
        }
        version.update { it + 1 }
    }

    suspend fun receivedBytes(transferId: UUID): UInt = mutex.withLock { received[transferId] ?: 0u }

    suspend fun finish(transferId: UUID) {
        mutex.withLock { received.remove(transferId) }
        version.update { it + 1 }
    }

    suspend fun waitForProgress(transferId: UUID, prior: UInt, timeoutMs: Long): Boolean =
        withTimeoutOrNull(timeoutMs) {
            version.first { receivedBytes(transferId) > prior }
            true
        } ?: false

    suspend fun awaitWindow(transferId: UUID, offset: Long, windowBytes: Long, timeoutMs: Long) {
        while (true) {
            val acked = receivedBytes(transferId).toLong()
            if (offset < acked + windowBytes) return
            if (!waitForProgress(transferId, acked.toUInt(), timeoutMs)) {
                finish(transferId)
                throw IOException("transfer stalled: fragment acks stopped at $offset")
            }
        }
    }
}
