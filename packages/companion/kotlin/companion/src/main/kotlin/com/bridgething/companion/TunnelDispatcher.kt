package com.bridgething.companion

import com.bridgething.gateway.BridgethingGateway
import com.bridgething.gateway.TunnelOpenHandle
import com.bridgething.gateway.device
import com.bridgething.gateway.tunnel
import com.bridgething.schema.Priority
import com.bridgething.schema.TunnelAck
import com.bridgething.schema.TunnelClosed
import com.bridgething.schema.TunnelData
import com.bridgething.schema.TunnelError
import com.bridgething.schema.TunnelErrorConnectFailedInner
import com.bridgething.schema.TunnelErrorReply
import com.bridgething.schema.TunnelOpen
import com.bridgething.schema.TunnelOpenReply
import java.net.InetSocketAddress
import java.net.Socket
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

public class TunnelDispatcher(
    private val connectTimeoutMs: Int = 15_000,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val mutex = Mutex()

    private var openJob: Job? = null
    private var dataJob: Job? = null
    private var ackJob: Job? = null
    private var closeJob: Job? = null

    private val sockets = ConcurrentHashMap<UUID, Socket>()
    private val pumps = ConcurrentHashMap<UUID, Job>()
    private val flushers = ConcurrentHashMap<UUID, Job>()
    private val delivered = ConcurrentHashMap<UUID, Long>()
    private val acks = TransferAckWindow()

    public suspend fun start(gateway: BridgethingGateway) {
        mutex.withLock {
            openJob?.cancel()
            dataJob?.cancel()
            ackJob?.cancel()
            closeJob?.cancel()

            openJob = scope.launch {
                gateway.tunnel.openRequests.collect { (handle, req) ->
                    launch { handleOpen(handle, req, gateway) }
                }
            }
            dataJob = scope.launch {
                gateway.tunnel.data.collect { (deviceId, msg) ->
                    launch { handleData(msg, deviceId, gateway) }
                }
            }
            ackJob = scope.launch {
                gateway.tunnel.ack.collect { (_, msg) ->
                    acks.note(msg.tunnelId, acks.receivedBytes(msg.tunnelId) + msg.consumed.toLong())
                }
            }
            closeJob = scope.launch {
                gateway.tunnel.close.collect { (_, msg) ->
                    handleClose(msg)
                }
            }
        }
    }

    public suspend fun stop() {
        mutex.withLock {
            openJob?.cancel(); openJob = null
            dataJob?.cancel(); dataJob = null
            ackJob?.cancel(); ackJob = null
            closeJob?.cancel(); closeJob = null
        }
        for ((_, pump) in pumps) pump.cancel()
        pumps.clear()
        for ((_, flusher) in flushers) flusher.cancel()
        flushers.clear()
        delivered.clear()
        for ((_, socket) in sockets) runCatching { socket.close() }
        sockets.clear()
    }

    private suspend fun handleOpen(handle: TunnelOpenHandle, req: TunnelOpen, gateway: BridgethingGateway) {
        val socket = try {
            withContext(Dispatchers.IO) {
                Socket().apply { connect(InetSocketAddress(req.host, req.port.toInt()), connectTimeoutMs) }
            }
        } catch (e: Throwable) {
            runCatching {
                handle.respondErr(
                    TunnelErrorReply(
                        TunnelError.ConnectFailed(TunnelErrorConnectFailedInner(reason = e.message ?: e.toString()))
                    )
                )
            }
            return
        }
        val id = req.tunnelId
        sockets[id] = socket
        runCatching { handle.respond(TunnelOpenReply) }
        pumps[id] = scope.launch { runPump(id, socket, gateway) }
        flushers[id] = scope.launch {
            while (true) {
                delay(ACK_FLUSH_MS)
                flushAck(id, handle.deviceId, gateway)
            }
        }
    }

    private suspend fun handleData(msg: TunnelData, deviceId: String, gateway: BridgethingGateway) {
        val socket = sockets[msg.tunnelId] ?: return
        val written = runCatching {
            val out = socket.getOutputStream()
            withContext(Dispatchers.IO) {
                out.write(msg.bytes)
                out.flush()
            }
            msg.bytes.size.toLong()
        }.getOrElse { return }
        val pending = delivered.merge(msg.tunnelId, written, Long::plus) ?: written
        if (pending >= ACK_INTERVAL_BYTES) flushAck(msg.tunnelId, deviceId, gateway)
    }

    private suspend fun flushAck(id: UUID, deviceId: String, gateway: BridgethingGateway) {
        if (!sockets.containsKey(id)) return
        val pending = delivered.put(id, 0L) ?: return
        if (pending <= 0L) return
        runCatching {
            gateway.device(deviceId).tunnel.ack(TunnelAck(tunnelId = id, consumed = pending.toUInt()))
        }
    }

    private fun handleClose(msg: TunnelClosed) {
        sockets.remove(msg.tunnelId)?.let { runCatching { it.close() } }
        pumps.remove(msg.tunnelId)?.cancel()
        flushers.remove(msg.tunnelId)?.cancel()
        delivered.remove(msg.tunnelId)
    }

    private suspend fun runPump(id: UUID, socket: Socket, gateway: BridgethingGateway) {
        val pacer = TransferPacer()
        var sent = 0L
        val buf = ByteArray(pacer.fragmentBytes)
        try {
            val input = socket.getInputStream()
            while (true) {
                pacer.observe(acks.receivedBytes(id))
                acks.awaitWindow(id, sent, pacer.windowBytes, ACK_STALL_MS)
                val n = withContext(Dispatchers.IO) { input.read(buf, 0, minOf(buf.size, pacer.fragmentBytes)) }
                if (n < 0) break // remote EOF
                if (n > 0) {
                    sent += n
                    runCatching {
                        gateway.tunnel.data(TunnelData(tunnelId = id, bytes = buf.copyOf(n)), priority = Priority.Bulk)
                    }
                }
            }
            finishRemote(id, reason = null, gateway = gateway)
        } catch (e: Throwable) {
            finishRemote(id, reason = e.message, gateway = gateway)
        } finally {
            pumps.remove(id)
            flushers.remove(id)?.cancel()
            delivered.remove(id)
            acks.finish(id)
        }
    }

    private suspend fun finishRemote(id: UUID, reason: String?, gateway: BridgethingGateway) {
        val socket = sockets.remove(id) ?: return
        runCatching { socket.close() }
        runCatching { gateway.tunnel.closed(TunnelClosed(tunnelId = id, reason = reason), priority = Priority.Bulk) }
    }

    public fun close() {
        scope.cancel()
    }

    private companion object {
        const val ACK_INTERVAL_BYTES = 16L * 1024
        const val ACK_FLUSH_MS = 300L
        const val ACK_STALL_MS = 30_000L
    }
}
