package dev.bridgething.gateway

import android.annotation.SuppressLint
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothSocket
import dev.bridgething.schema.BridgethingProtocol
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

    private val incomingEvents: Channel<AdapterEvent> = Channel(Channel.UNLIMITED)
    override val events: Flow<AdapterEvent> = incomingEvents.consumeAsFlow()

    override suspend fun start() {
        // Discovery is the consumer's responsibility - there's no scanOn API
        // here. Call [connect] for each bonded device the consumer wants to
        // bridge.
    }

    override suspend fun stop() {
        val toClose: List<Session>
        mutex.withLock {
            toClose = sessions.values.toList()
            sessions.clear()
        }
        toClose.forEach { it.close() }
        incomingEvents.close()
        ioScope.cancel()
    }

    override suspend fun disconnect(deviceId: String) {
        val session = mutex.withLock { sessions.remove(deviceId) }
            ?: throw AdapterException.UnknownDevice(deviceId)
        session.close()
        incomingEvents.send(AdapterEvent.Disconnected(deviceId))
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
    public suspend fun connect(device: BluetoothDevice): Device = withContext(Dispatchers.IO) {
        val deviceId = device.address
        val deviceName = try {
            device.name ?: deviceId
        } catch (_: SecurityException) {
            deviceId
        }
        val info = Device(id = deviceId, name = deviceName)

        val socket: BluetoothSocket = try {
            device.createRfcommSocketToServiceRecord(serviceUuid)
        } catch (e: IOException) {
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
            throw AdapterException.TransportFailure("rfcomm connect to $deviceId failed: ${e.message}")
        }

        val session = Session(this@BluetoothSocketAdapter, info, socket)
        mutex.withLock { sessions[deviceId] = session }
        session.start()
        incomingEvents.send(AdapterEvent.Connected(info))
        info
    }

    internal suspend fun emitBytes(deviceId: String, bytes: ByteArray) {
        incomingEvents.send(AdapterEvent.Bytes(deviceId, bytes))
    }

    internal suspend fun emitDisconnected(deviceId: String) {
        mutex.withLock { sessions.remove(deviceId) }
        incomingEvents.send(AdapterEvent.Disconnected(deviceId))
    }
}

private class Session(
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
                owner.emitDisconnected(device.id)
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
