package com.bridgething.gateway

import android.annotation.SuppressLint
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothSocket
import android.util.Log
import com.bridgething.schema.BridgethingProtocol
import kotlinx.coroutines.CoroutineName
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
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
) : Adapter {

    internal val ioScope: CoroutineScope =
        CoroutineScope(SupervisorJob() + Dispatchers.IO + CoroutineName("bridgething-bt"))

    private val mutex = Mutex()
    private val sessions = mutableMapOf<String, Session>()
    private val devices = mutableMapOf<String, BluetoothDevice>()
    private val reconnectJobs = mutableMapOf<String, Job>()
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
        }
        toClose.forEach { it.close() }
        incomingEvents.close()
        ioScope.cancel()
    }

    override suspend fun disconnect(deviceId: String) {
        val session = mutex.withLock {
            devices.remove(deviceId)
            reconnectJobs.remove(deviceId)?.cancel()
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
     */
    @SuppressLint("MissingPermission")
    public suspend fun connect(device: BluetoothDevice, scheduleOnFailure: Boolean = true): Device = withContext(Dispatchers.IO) {
        val deviceId = device.address
        mutex.withLock {
            devices[deviceId] = device
            sessions[deviceId]
        }?.let { return@withContext it.device }

        val deviceName = try {
            device.name ?: deviceId
        } catch (_: SecurityException) {
            deviceId
        }
        val info = Device(id = deviceId, name = deviceName)

        Log.i(TAG, "opening rfcomm to $deviceId ($deviceName) uuid=$serviceUuid bonded=${device.bondState == BluetoothDevice.BOND_BONDED}")

        val socket: BluetoothSocket = try {
            device.createRfcommSocketToServiceRecord(serviceUuid)
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
            val bonded = device.bondState == BluetoothDevice.BOND_BONDED
            Log.w(TAG, "rfcomm connect to $deviceId failed (aclConnected=$aclUp bonded=$bonded): ${e.message}")
            // A first connect to an unbonded Car Thing is expected to fail - it's
            // what kicks off Android pairing. Retry so a later attempt lands once
            // the bond completes. Only surface LinkFailed when we're already
            // bonded AND link-connected but the daemon still won't answer; that's
            // the real "can't reach the daemon" case, not pairing-in-progress.
            if (aclUp && bonded) {
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
                if (runCatching { connect(device, scheduleOnFailure = false) }.isSuccess) return@launch
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
