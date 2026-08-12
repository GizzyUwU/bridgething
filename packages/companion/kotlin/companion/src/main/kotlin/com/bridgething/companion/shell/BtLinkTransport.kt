package com.bridgething.companion.shell

import android.annotation.SuppressLint
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothSocket
import android.util.Log
import java.io.IOException
import java.util.UUID
import kotlinx.coroutines.CoroutineName
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.ClosedReceiveChannelException
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import uniffi.bridgething_companion.LinkDevice
import uniffi.bridgething_companion.LinkInbox
import uniffi.bridgething_companion.LinkTransport

public sealed class BtLinkException(message: String) : Exception(message) {
    public class NotBonded(deviceId: String) : BtLinkException("not bonded: $deviceId")
    public class TransportFailure(message: String) : BtLinkException(message)
}

public class BtLinkTransport(
    private val serviceUuid: UUID = PROFILE_UUID,
    private val bluetooth: BluetoothAdapter? = null,
) : LinkTransport {

    internal val ioScope: CoroutineScope =
        CoroutineScope(SupervisorJob() + Dispatchers.IO + CoroutineName("bridgething-bt"))

    private val mutex = Mutex()

    private val sessions = java.util.concurrent.ConcurrentHashMap<String, Session>()
    private val devices = mutableMapOf<String, BluetoothDevice>()
    private val reconnectJobs = mutableMapOf<String, Job>()
    private val pending = mutableMapOf<String, Deferred<LinkDevice>>()
    private val linkFailedReported = mutableSetOf<String>()

    @Volatile private var inbox: LinkInbox? = null

    @Volatile private var stopped = false

    override fun maxBatchBytes(): UInt = MAX_BATCH_BYTES

    override fun start(inbox: LinkInbox) {
        stopped = false
        this.inbox = inbox
    }

    override fun stop() {
        stopped = true
        inbox = null
        runBlocking {
            val toClose: List<Session>
            mutex.withLock {
                toClose = sessions.values.toList()
                sessions.clear()
                devices.clear()
                reconnectJobs.values.forEach { it.cancel() }
                reconnectJobs.clear()
                pending.values.forEach { it.cancel() }
                pending.clear()
                linkFailedReported.clear()
            }
            toClose.forEach { it.close() }
        }
    }

    override fun send(deviceId: String, batch: ByteArray) {
        val session = sessions[deviceId]
        if (session == null || !session.write(batch)) {
            inbox?.onSendFailed(deviceId)
        }
    }

    override fun disconnect(deviceId: String) {
        ioScope.launch {
            val session = mutex.withLock {
                devices.remove(deviceId)
                reconnectJobs.remove(deviceId)?.cancel()
                pending.remove(deviceId)?.cancel()
                linkFailedReported.remove(deviceId)
                sessions.remove(deviceId)
            } ?: return@launch
            session.close()
            inbox?.onDisconnected(deviceId)
        }
    }

    override fun reconnect(deviceId: String) {
        ioScope.launch {
            val device = mutex.withLock { devices[deviceId] } ?: return@launch
            runCatching { connect(device) }
        }
    }

    public suspend fun connect(device: BluetoothDevice, scheduleOnFailure: Boolean = true): LinkDevice {
        val deviceId = device.address

        val attempt = mutex.withLock {
            sessions[deviceId]?.let { return it.device }
            devices[deviceId] = device
            pending[deviceId] ?: ioScope.async { openSession(device, scheduleOnFailure) }
                .also { pending[deviceId] = it }
        }
        try {
            return attempt.await()
        } finally {
            mutex.withLock { if (pending[deviceId] === attempt) pending.remove(deviceId) }
        }
    }

    @SuppressLint("MissingPermission")
    private suspend fun openSession(device: BluetoothDevice, scheduleOnFailure: Boolean): LinkDevice =
        withContext(Dispatchers.IO) {
            val deviceId = device.address
            val deviceName = try {
                device.name ?: deviceId
            } catch (_: SecurityException) {
                deviceId
            }
            val info = LinkDevice(id = deviceId, name = deviceName)

            if (device.bondState != BluetoothDevice.BOND_BONDED) {
                Log.i(TAG, "refusing rfcomm to $deviceId: not bonded (bondState=${device.bondState})")
                throw BtLinkException.NotBonded(deviceId)
            }

            bluetooth?.let { ba ->
                if (runCatching { ba.isDiscovering }.getOrDefault(false)) {
                    Log.i(TAG, "cancelling discovery before connecting $deviceId")
                    runCatching { ba.cancelDiscovery() }
                }
            }

            Log.i(TAG, "opening rfcomm to $deviceId ($deviceName) uuid=$serviceUuid")

            val socket: BluetoothSocket = try {
                device.createInsecureRfcommSocketToServiceRecord(serviceUuid)
            } catch (e: IOException) {
                Log.w(TAG, "createRfcommSocket for $deviceId failed: ${e.message}")
                throw BtLinkException.TransportFailure("createRfcommSocket for $deviceId failed: ${e.message}")
            } ?: throw BtLinkException.TransportFailure("createRfcommSocket returned null for $deviceId")

            try {
                socket.connect()
            } catch (e: IOException) {
                runCatching { socket.close() }
                Log.w(TAG, "rfcomm connect to $deviceId failed (aclConnected=${isAclConnected(device)}): ${e.message}")
                if (scheduleOnFailure && !stopped && mutex.withLock { devices.containsKey(deviceId) }) {
                    scheduleReconnect(deviceId, device)
                }
                throw BtLinkException.TransportFailure("rfcomm connect to $deviceId failed: ${e.message}")
            }
            Log.i(TAG, "rfcomm connected to $deviceId")

            val session = Session(this@BtLinkTransport, info, socket)
            val installed = mutex.withLock {
                if (sessions.containsKey(deviceId)) {
                    false
                } else {
                    sessions[deviceId] = session
                    linkFailedReported.remove(deviceId)
                    true
                }
            }
            if (!installed) {
                runCatching { socket.close() }
                return@withContext mutex.withLock { sessions[deviceId] }?.device ?: info
            }
            session.start()
            inbox?.onConnected(info)
            info
        }

    public suspend fun forget(deviceId: String) {
        val session = mutex.withLock {
            devices.remove(deviceId)
            reconnectJobs.remove(deviceId)?.cancel()
            pending.remove(deviceId)?.cancel()
            linkFailedReported.remove(deviceId)
            sessions.remove(deviceId)
        } ?: return
        session.close()
        inbox?.onDisconnected(deviceId)
    }

    internal fun emitBytes(deviceId: String, bytes: ByteArray) {
        inbox?.onBytes(deviceId, bytes)
    }

    internal fun emitWriteComplete(deviceId: String) {
        inbox?.onWriteComplete(deviceId)
    }

    internal suspend fun emitDisconnected(deviceId: String, session: Session) {
        val device = mutex.withLock {
            if (sessions[deviceId] !== session) return@withLock null
            sessions.remove(deviceId)
            devices[deviceId]
        } ?: return
        inbox?.onDisconnected(deviceId)
        if (!stopped) scheduleReconnect(deviceId, device)
    }

    private suspend fun scheduleReconnect(deviceId: String, device: BluetoothDevice) {
        val job = ioScope.launch {
            var delayMs = RECONNECT_BASE_MS
            var attempts = 0
            while (isActive) {
                if (mutex.withLock { sessions.containsKey(deviceId) || !devices.containsKey(deviceId) }) return@launch
                delay(delayMs)
                if (mutex.withLock { sessions.containsKey(deviceId) || !devices.containsKey(deviceId) }) return@launch
                val outcome = runCatching { connect(device, scheduleOnFailure = false) }
                if (outcome.isSuccess) return@launch
                if (outcome.exceptionOrNull() is BtLinkException.NotBonded) {
                    Log.i(TAG, "reconnect loop for $deviceId stopping: not bonded")
                    return@launch
                }
                attempts += 1
                announceLinkFailedOnce(deviceId, device, attempts)
                delayMs = (delayMs * 2).coerceAtMost(RECONNECT_MAX_MS)
            }
        }
        mutex.withLock {
            reconnectJobs.remove(deviceId)?.cancel()
            reconnectJobs[deviceId] = job
        }
    }

    @SuppressLint("MissingPermission")
    private suspend fun announceLinkFailedOnce(deviceId: String, device: BluetoothDevice, attempts: Int) {
        if (attempts < RECONNECT_ATTEMPTS_BEFORE_ANNOUNCE || !isAclConnected(device)) return
        if (!mutex.withLock { linkFailedReported.add(deviceId) }) return
        val deviceName = try {
            device.name ?: deviceId
        } catch (_: SecurityException) {
            deviceId
        }
        Log.w(TAG, "link failed for $deviceId after $attempts attempts; continuing slow retry")
        inbox?.onLinkFailed(deviceId, deviceName, "rfcomm link to $deviceId did not come up after $attempts attempts")
    }

    private fun isAclConnected(device: BluetoothDevice): Boolean = try {
        val method = BluetoothDevice::class.java.getMethod("isConnected")
        method.invoke(device) as? Boolean ?: false
    } catch (_: Throwable) {
        false
    }

    public companion object {
        public val PROFILE_UUID: UUID = UUID.fromString("dead0000-854d-408e-81f0-fb6147f918fd")

        private const val TAG = "BridgethingBT"
        private const val RECONNECT_BASE_MS = 1_000L
        private const val RECONNECT_MAX_MS = 30_000L
        private const val RECONNECT_ATTEMPTS_BEFORE_ANNOUNCE = 6
        private val MAX_BATCH_BYTES: UInt = 16u * 1024u
    }
}

internal class Session(
    val owner: BtLinkTransport,
    val device: LinkDevice,
    val socket: BluetoothSocket,
) {
    private var readJob: Job? = null
    private var writerJob: Job? = null

    private val writes = Channel<ByteArray>(Channel.UNLIMITED)

    fun start() {
        readJob = owner.ioScope.launch {
            val input = socket.inputStream
            val buf = ByteArray(4096)
            try {
                while (isActive) {
                    val n = input.read(buf)
                    if (n < 0) break
                    if (n == 0) continue
                    owner.emitBytes(device.id, buf.copyOf(n))
                }
            } catch (_: IOException) {
                // stream is dead; disconnect is routed in finally.
            } finally {
                owner.emitDisconnected(device.id, this@Session)
                runCatching { socket.close() }
            }
        }
        writerJob = owner.ioScope.launch { writerLoop() }
    }

    fun write(batch: ByteArray): Boolean = writes.trySend(batch).isSuccess

    internal suspend fun writerLoop() {
        val out = socket.outputStream
        try {
            for (batch in writes) {
                withContext(Dispatchers.IO) {
                    out.write(batch)
                    out.flush()
                }
                owner.emitWriteComplete(device.id)
            }
        } catch (_: IOException) {
            // socket is dead; the read loop routes the disconnect.
        } catch (_: ClosedReceiveChannelException) {
            // channel closed on teardown.
        }
    }

    suspend fun close() {
        readJob?.cancel()
        writerJob?.cancel()
        writes.close()
        runCatching { socket.close() }
    }
}
