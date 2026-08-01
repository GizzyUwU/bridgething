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
    private val received = mutableMapOf<UUID, Long>()
    private val version = MutableStateFlow(0L)

    suspend fun note(transferId: UUID, receivedBytes: Long) {
        mutex.withLock {
            val prior = received[transferId] ?: 0L
            if (receivedBytes <= prior) return
            received[transferId] = receivedBytes
        }
        version.update { it + 1 }
    }

    suspend fun receivedBytes(transferId: UUID): Long = mutex.withLock { received[transferId] ?: 0L }

    suspend fun finish(transferId: UUID) {
        mutex.withLock { received.remove(transferId) }
        version.update { it + 1 }
    }

    suspend fun waitForProgress(transferId: UUID, prior: Long, timeoutMs: Long): Boolean =
        withTimeoutOrNull(timeoutMs) {
            version.first { receivedBytes(transferId) > prior }
            true
        } ?: false

    suspend fun awaitWindow(transferId: UUID, offset: Long, windowBytes: Long, timeoutMs: Long) {
        while (true) {
            val acked = receivedBytes(transferId)
            if (offset < acked + windowBytes) return
            if (!waitForProgress(transferId, acked, timeoutMs)) {
                finish(transferId)
                throw IOException("transfer stalled: fragment acks stopped at $offset")
            }
        }
    }
}
