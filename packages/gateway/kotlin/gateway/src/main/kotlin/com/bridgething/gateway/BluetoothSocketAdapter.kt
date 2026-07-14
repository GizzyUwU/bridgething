package com.bridgething.gateway

import android.annotation.SuppressLint
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothSocket
import android.util.Log
import com.bridgething.schema.BridgethingProtocol
import kotlinx.coroutines.CoroutineName
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.ClosedReceiveChannelException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.consumeAsFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.selects.select
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import java.io.IOException
import java.util.UUID

/**
 * `Adapter` implementation that talks to the bridgething daemon over a
 * BR/EDR RFCOMM channel published by the daemon under
 * [BridgethingProtocol.PROFILE_UUID].
 *
 * Lifecycle:
 * 1. The user pairs the Car Thing once via system Bluetooth settings (handled
 *    out of band - Android exposes no library API for non-interactive
 *    pairing).
 * 2. The consuming app obtains a [BluetoothDevice] for the bonded Car Thing
 *    (typically by walking `BluetoothAdapter.bondedDevices`).
 * 3. Call [connect] to open the RFCOMM session - this wires up an inbound
 *    read loop and starts emitting [AdapterEvent.Bytes].
 * 4. Long-running connections must be hosted in a foreground service on
 *    Android 8+ (notification config is the consumer's responsibility).
 *
 * Thread model: one read coroutine per session running on `Dispatchers.IO`
 * blocks on `InputStream.read`; writes hop to `Dispatchers.IO` and serialize
 * per-session through a [Mutex]. Public methods are safe to call from any
 * dispatcher.
 *
 * Permissions: declared in the library manifest (`BLUETOOTH_CONNECT`,
 * `BLUETOOTH_SCAN`, plus legacy `BLUETOOTH` / `BLUETOOTH_ADMIN`) so they
 * merge into the consumer's manifest. The consumer is still responsible for
 * runtime permission grants on Android 12+.
 */
public class BluetoothSocketAdapter(
    private val serviceUuid: UUID = BridgethingProtocol.PROFILE_UUID,
    private val bluetooth: BluetoothAdapter? = null,
) : Adapter {

    internal val ioScope: CoroutineScope =
        CoroutineScope(SupervisorJob() + Dispatchers.IO + CoroutineName("bridgething-bt"))

    private val mutex = Mutex()
    private val sessions = mutableMapOf<String, Session>()
    private val devices = mutableMapOf<String, BluetoothDevice>()
    private val reconnectJobs = mutableMapOf<String, Job>()
    private val pending = mutableMapOf<String, Deferred<Device>>()
    @Volatile private var stopped = false

    private val incomingEvents: Channel<AdapterEvent> = Channel(Channel.UNLIMITED)
    override val events: Flow<AdapterEvent> = incomingEvents.consumeAsFlow()

    override suspend fun start() {
        // Discovery is the consumer's responsibility - there's no scanOn API
        // here. Call [connect] for each bonded device the consumer wants to
        // bridge.
    }

    override suspend fun stop() {
        stopped = true
        val toClose: List<Session>
        mutex.withLock {
            toClose = sessions.values.toList()
            sessions.clear()
            devices.clear()
            reconnectJobs.values.forEach { it.cancel() }
            reconnectJobs.clear()
            pending.values.forEach { it.cancel() }
            pending.clear()
        }
        toClose.forEach { it.close() }
        incomingEvents.close()
        ioScope.cancel()
    }

    override suspend fun disconnect(deviceId: String) {
        val session = mutex.withLock {
            devices.remove(deviceId)
            reconnectJobs.remove(deviceId)?.cancel()
            pending.remove(deviceId)?.cancel()
            sessions.remove(deviceId)
        } ?: throw AdapterException.UnknownDevice(deviceId)
        session.close()
        incomingEvents.send(AdapterEvent.Disconnected(deviceId))
    }

    override suspend fun reconnect(deviceId: String) {
        val device = mutex.withLock { devices[deviceId] } ?: return
        runCatching { connect(device) }
    }

    override suspend fun send(deviceId: String, frame: ByteArray) {
        val session = mutex.withLock { sessions[deviceId] }
            ?: throw AdapterException.UnknownDevice(deviceId)
        session.write(frame)
    }

    /**
     * Open an RFCOMM session to a bonded BluetoothDevice and begin pumping its
     * bytes into the [events] flow. Must be invoked with the
     * `BLUETOOTH_CONNECT` runtime permission already granted.
     *
     * Concurrent callers for one device share a single attempt. [connect] is
     * driven from four independent places - the reconnect loop, CDM presence
     * wakeups, the foreground service's START_STICKY restarts and the pair flow
     * - and two RFCOMM connects racing on the same device make the stack
     * renegotiate link security mid-flight, which the OS can escalate into a
     * pairing dialog on an already-bonded peer.
     *
     * Unbonded devices are refused with [AdapterException.NotBonded] instead of
     * being connected: an RFCOMM connect to an unbonded peer is *how* Android
     * starts pairing, so a retry loop doing it spawns one system dialog per
     * attempt. Bonding is the caller's job - explicit, foregrounded, once.
     */
    public suspend fun connect(device: BluetoothDevice, scheduleOnFailure: Boolean = true): Device {
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
    private suspend fun openSession(device: BluetoothDevice, scheduleOnFailure: Boolean): Device =
        withContext(Dispatchers.IO) {
            val deviceId = device.address
            val deviceName = try {
                device.name ?: deviceId
            } catch (_: SecurityException) {
                deviceId
            }
            val info = Device(id = deviceId, name = deviceName)

            if (device.bondState != BluetoothDevice.BOND_BONDED) {
                Log.i(TAG, "refusing rfcomm to $deviceId: not bonded (bondState=${device.bondState})")
                throw AdapterException.NotBonded(deviceId)
            }

            // Discovery hogs the baseband and makes socket connects slow and
            // flaky; Android's own docs require cancelling it before connecting.
            // Needs BLUETOOTH_SCAN on S+, which we may not hold - best effort.
            bluetooth?.let { ba ->
                if (runCatching { ba.isDiscovering }.getOrDefault(false)) {
                    Log.i(TAG, "cancelling discovery before connecting $deviceId")
                    runCatching { ba.cancelDiscovery() }
                }
            }

            Log.i(TAG, "opening rfcomm to $deviceId ($deviceName) uuid=$serviceUuid")

            val socket: BluetoothSocket = try {
                // Insecure on purpose. The daemon registers this profile with
                // require_authentication=false, so the secure variant demands an
                // authenticated link the peer never asked for - and every connect
                // that re-negotiates authentication is a chance for the stack to
                // decide the link key is unusable and pop a pairing dialog on a
                // perfectly healthy bond. Pairing still happens once, explicitly,
                // in the pair flow; a bonded link stays encrypted.
                device.createInsecureRfcommSocketToServiceRecord(serviceUuid)
            } catch (e: IOException) {
                Log.w(TAG, "createRfcommSocket for $deviceId failed: ${e.message}")
                throw AdapterException.TransportFailure(
                    "createRfcommSocket for $deviceId failed: ${e.message}"
                )
            } ?: throw AdapterException.TransportFailure(
                "createRfcommSocket returned null for $deviceId"
            )

            try {
                socket.connect()
            } catch (e: IOException) {
                runCatching { socket.close() }
                val aclUp = isAclConnected(device)
                Log.w(TAG, "rfcomm connect to $deviceId failed (aclConnected=$aclUp): ${e.message}")
                // We only get here bonded, so a link-level connection that still
                // won't carry the service is the real "daemon unreachable" case.
                if (aclUp) {
                    incomingEvents.send(
                        AdapterEvent.LinkFailed(info, "rfcomm connect to $deviceId failed: ${e.message}")
                    )
                }
                if (scheduleOnFailure && !stopped && mutex.withLock { devices.containsKey(deviceId) }) {
                    scheduleReconnect(deviceId, device)
                }
                throw AdapterException.TransportFailure("rfcomm connect to $deviceId failed: ${e.message}")
            }
            Log.i(TAG, "rfcomm connected to $deviceId")

            val session = Session(this@BluetoothSocketAdapter, info, socket)
            val installed = mutex.withLock {
                if (sessions.containsKey(deviceId)) {
                    false
                } else {
                    sessions[deviceId] = session
                    true
                }
            }
            if (!installed) {
                runCatching { socket.close() }
                return@withContext mutex.withLock { sessions[deviceId] }?.device ?: info
            }
            session.start()
            incomingEvents.send(AdapterEvent.Connected(info))
            info
        }

    /**
     * Drop [deviceId] completely: close any session, stop its reconnect loop and
     * forget it, so nothing reconnects until someone connects it again. Unlike
     * [disconnect] this is a no-op on an unknown device.
     *
     * The bond-state watcher calls this when a peer loses its bond, so the retry
     * loop cannot sit there hammering an unbonded device.
     */
    public suspend fun forget(deviceId: String) {
        val session = mutex.withLock {
            devices.remove(deviceId)
            reconnectJobs.remove(deviceId)?.cancel()
            pending.remove(deviceId)?.cancel()
            sessions.remove(deviceId)
        } ?: return
        session.close()
        incomingEvents.send(AdapterEvent.Disconnected(deviceId))
    }

    internal suspend fun emitBytes(deviceId: String, bytes: ByteArray) {
        incomingEvents.send(AdapterEvent.Bytes(deviceId, bytes))
    }

    internal suspend fun emitDisconnected(deviceId: String, session: Session) {
        val device = mutex.withLock {
            if (sessions[deviceId] !== session) return@withLock null
            sessions.remove(deviceId)
            devices[deviceId]
        } ?: return
        incomingEvents.send(AdapterEvent.Disconnected(deviceId))
        if (!stopped) scheduleReconnect(deviceId, device)
    }

    private suspend fun scheduleReconnect(deviceId: String, device: BluetoothDevice) {
        val job = ioScope.launch {
            var delayMs = RECONNECT_BASE_MS
            while (isActive) {
                if (mutex.withLock { sessions.containsKey(deviceId) || !devices.containsKey(deviceId) }) return@launch
                kotlinx.coroutines.delay(delayMs)
                if (mutex.withLock { sessions.containsKey(deviceId) || !devices.containsKey(deviceId) }) return@launch
                val outcome = runCatching { connect(device, scheduleOnFailure = false) }
                if (outcome.isSuccess) return@launch
                // Unbonded is terminal, never retried: retrying an unbonded connect
                // is exactly what re-triggers Android pairing, once per attempt.
                // Whoever re-bonds the device - the pair flow, or the user in system
                // settings - drives the next connect through the bond-state watcher.
                if (outcome.exceptionOrNull() is AdapterException.NotBonded) {
                    Log.i(TAG, "reconnect loop for $deviceId stopping: not bonded")
                    return@launch
                }
                delayMs = (delayMs * 2).coerceAtMost(RECONNECT_MAX_MS)
            }
        }
        mutex.withLock {
            reconnectJobs.remove(deviceId)?.cancel()
            reconnectJobs[deviceId] = job
        }
    }

    /**
     * Whether the phone currently holds a baseband (ACL) link to [device].
     * There is no public API for classic-Bluetooth connection state, so this
     * reflects the hidden `BluetoothDevice.isConnected()`. Defaults to `false`
     * if reflection is unavailable - we'd rather under-report link failures
     * than raise false ones.
     */
    private fun isAclConnected(device: BluetoothDevice): Boolean = try {
        val method = BluetoothDevice::class.java.getMethod("isConnected")
        method.invoke(device) as? Boolean ?: false
    } catch (_: Throwable) {
        false
    }

    private companion object {
        const val TAG = "BridgethingBT"
        const val RECONNECT_BASE_MS = 1_000L
        const val RECONNECT_MAX_MS = 30_000L
    }
}

internal class Session(
    val owner: BluetoothSocketAdapter,
    val device: Device,
    val socket: BluetoothSocket,
) {
    private var readJob: Job? = null
    private var writerJob: Job? = null
    private val normalLane = Channel<ByteArray>(Channel.UNLIMITED)
    private val bulkLane = Channel<ByteArray>(BULK_LANE_DEPTH)
    private val backgroundLane = Channel<ByteArray>(BULK_LANE_DEPTH)

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

    suspend fun write(frame: ByteArray) {
        when (if (frame.size >= 16) frame[5] else 0x00.toByte()) {
            BULK_BYTE -> bulkLane.send(frame)
            BACKGROUND_BYTE -> backgroundLane.send(frame)
            else -> normalLane.send(frame)
        }
    }

    private suspend fun writerLoop() {
        val out = socket.outputStream
        try {
            while (true) {
                val frame = normalLane.tryReceive().getOrNull()
                    ?: bulkLane.tryReceive().getOrNull()
                    ?: select {
                        normalLane.onReceive { it }
                        bulkLane.onReceive { it }
                        backgroundLane.onReceive { it }
                    }
                withContext(Dispatchers.IO) {
                    out.write(frame)
                    out.flush()
                }
            }
        } catch (_: IOException) {
            // socket is dead; the read loop routes the disconnect.
        } catch (_: ClosedReceiveChannelException) {
            // lanes closed on teardown.
        }
    }

    suspend fun close() {
        readJob?.cancel()
        writerJob?.cancel()
        normalLane.close()
        bulkLane.close()
        backgroundLane.close()
        runCatching { socket.close() }
    }

    private companion object {
        const val BULK_LANE_DEPTH = 4
        const val BULK_BYTE: Byte = 0x01
        const val BACKGROUND_BYTE: Byte = 0x02
    }
}
