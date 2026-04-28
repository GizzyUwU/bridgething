package com.bridgething.adapter

import android.annotation.SuppressLint
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothSocket
import android.content.Context
import com.facebook.proguard.annotations.DoNotStrip
import com.margelo.nitro.NitroModules
import com.margelo.nitro.bridgething.adapter.BridgethingTransportDevice
import com.margelo.nitro.bridgething.adapter.HybridBridgethingTransportSpec
import com.margelo.nitro.core.ArrayBuffer
import com.margelo.nitro.core.Promise
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.nio.ByteBuffer
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CoroutineName
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * Android-side bridgething transport. Owns one [BluetoothSocket] per active
 * RFCOMM session and pumps raw bytes back to JS via the Nitro callbacks set
 * by the TS adapter.
 *
 * Discovery is the host app's responsibility - Android exposes no
 * non-interactive pairing API. The app pairs the bridgething device once
 * via system Settings, then calls [connect] with the bonded MAC address.
 *
 * Long-running connections must be hosted in a foreground service on
 * Android 8+ (notification config is the consumer's responsibility - the
 * adapter is host-agnostic).
 *
 * Threading: one read coroutine per session blocks on [InputStream.read] on
 * `Dispatchers.IO`; writes serialize through a per-session [Mutex] on the
 * same dispatcher. Public methods are safe to call from any dispatcher.
 *
 * Permissions: declared in this library's manifest (`BLUETOOTH_CONNECT`,
 * `BLUETOOTH_SCAN`, plus legacy `BLUETOOTH`/`BLUETOOTH_ADMIN`) so they merge
 * into the consumer's manifest. The consumer is still responsible for
 * runtime permission grants on Android 12+.
 */
@DoNotStrip
class HybridBridgethingTransport : HybridBridgethingTransportSpec() {
    /// The bridgething RFCOMM service UUID. Daemon side advertises this same
    /// SDP record so we can both reach it from `BluetoothDevice` and validate
    /// that a bonded device actually speaks our protocol.
    private val serviceUuid: UUID = UUID.fromString("dead0000-53e5-4085-a5d8-f55f3f14ac5a")

    private val ioScope: CoroutineScope =
        CoroutineScope(SupervisorJob() + Dispatchers.IO + CoroutineName("bridgething-rn-transport"))

    private val sessions: ConcurrentHashMap<String, Session> = ConcurrentHashMap()
    private var started: Boolean = false

    private var onConnected: ((BridgethingTransportDevice) -> Unit)? = null
    private var onDisconnected: ((String) -> Unit)? = null
    private var onBytes: ((String, ArrayBuffer) -> Unit)? = null
    private var onError: ((String, String) -> Unit)? = null

    // MARK: - Hybrid spec

    override fun start(): Promise<Unit> = Promise.async {
        started = true
    }

    override fun stop(): Promise<Unit> = Promise.async {
        started = false
        val toClose = sessions.values.toList()
        sessions.clear()
        for (session in toClose) {
            session.close()
            onDisconnected?.invoke(session.device.id)
        }
    }

    @SuppressLint("MissingPermission")
    override fun connect(deviceId: String): Promise<BridgethingTransportDevice> = Promise.async {
        if (!started) throw RuntimeException("transport not started - call start() first")

        sessions[deviceId]?.let { return@async it.device }

        val context = NitroModules.applicationContext
            ?: throw RuntimeException("application context not available")
        val manager = context.getSystemService(Context.BLUETOOTH_SERVICE) as? BluetoothManager
            ?: throw RuntimeException("BluetoothManager unavailable")
        val adapter: BluetoothAdapter = manager.adapter
            ?: throw RuntimeException("device has no bluetooth adapter")

        val btDevice: BluetoothDevice = adapter.bondedDevices.firstOrNull { it.address == deviceId }
            ?: throw RuntimeException(
                "device $deviceId is not bonded - pair via system Settings before connecting"
            )

        val resolvedName: String = try {
            btDevice.name ?: deviceId
        } catch (_: SecurityException) {
            deviceId
        }
        val record = BridgethingTransportDevice(deviceId, resolvedName)

        val socket: BluetoothSocket = try {
            btDevice.createRfcommSocketToServiceRecord(serviceUuid)
        } catch (e: IOException) {
            throw RuntimeException("createRfcommSocket for $deviceId failed: ${e.message}")
        } ?: throw RuntimeException("createRfcommSocket returned null for $deviceId")

        try {
            withContext(Dispatchers.IO) { socket.connect() }
        } catch (e: IOException) {
            runCatching { socket.close() }
            throw RuntimeException("rfcomm connect to $deviceId failed: ${e.message}")
        }

        val session = Session(this@HybridBridgethingTransport, record, socket)
        sessions[record.id] = session
        session.start()
        onConnected?.invoke(record)
        record
    }

    override fun disconnect(deviceId: String): Promise<Unit> = Promise.async {
        val session = sessions.remove(deviceId)
            ?: throw RuntimeException("unknown device $deviceId")
        session.close()
        onDisconnected?.invoke(deviceId)
    }

    override fun send(deviceId: String, frame: ArrayBuffer): Promise<Unit> = Promise.async {
        val session = sessions[deviceId]
            ?: throw RuntimeException("unknown device $deviceId")
        // ArrayBuffer is non-owning across the boundary; copy into a ByteArray so
        // the outbound write can run after this Promise returns.
        val bytes = frame.getBuffer(copyIfNeeded = true).let { byteBuffer ->
            ByteArray(byteBuffer.remaining()).also { byteBuffer.get(it) }
        }
        session.write(bytes)
    }

    override fun getKnownDevices(): Array<BridgethingTransportDevice> {
        val context = NitroModules.applicationContext ?: return emptyArray()
        val manager = context.getSystemService(Context.BLUETOOTH_SERVICE) as? BluetoothManager
            ?: return emptyArray()
        val adapter: BluetoothAdapter = manager.adapter ?: return emptyArray()
        return try {
            @SuppressLint("MissingPermission")
            adapter.bondedDevices.map { device ->
                val name = try {
                    device.name ?: device.address
                } catch (_: SecurityException) {
                    device.address
                }
                BridgethingTransportDevice(device.address, name)
            }.toTypedArray()
        } catch (_: SecurityException) {
            emptyArray()
        }
    }

    override fun setOnConnected(callback: (device: BridgethingTransportDevice) -> Unit) {
        onConnected = callback
    }

    override fun setOnDisconnected(callback: (deviceId: String) -> Unit) {
        onDisconnected = callback
    }

    override fun setOnBytes(callback: (deviceId: String, frame: ArrayBuffer) -> Unit) {
        onBytes = callback
    }

    override fun setOnError(callback: (deviceId: String, description: String) -> Unit) {
        onError = callback
    }

    // MARK: - internal session plumbing

    internal fun emitBytes(deviceId: String, bytes: ByteArray) {
        val cb = onBytes ?: return
        val direct = ByteBuffer.allocateDirect(bytes.size).put(bytes)
        direct.flip()
        cb(deviceId, ArrayBuffer.wrap(direct))
    }

    internal fun emitDisconnected(deviceId: String) {
        sessions.remove(deviceId)
        onDisconnected?.invoke(deviceId)
    }

    internal fun emitError(deviceId: String, description: String) {
        onError?.invoke(deviceId, description)
    }

    internal val scope: CoroutineScope get() = ioScope

    protected fun finalize() {
        ioScope.cancel()
    }
}

private class Session(
    val owner: HybridBridgethingTransport,
    val device: BridgethingTransportDevice,
    val socket: BluetoothSocket,
) {
    private val writeMutex = Mutex()
    private var readJob: Job? = null
    @Volatile
    private var closed: Boolean = false

    fun start() {
        readJob = owner.scope.launch {
            val input: InputStream = socket.inputStream
            val buf = ByteArray(4096)
            try {
                while (isActive && !closed) {
                    val n = input.read(buf)
                    if (n < 0) break
                    if (n == 0) continue
                    owner.emitBytes(device.id, buf.copyOf(n))
                }
            } catch (e: IOException) {
                if (!closed) owner.emitError(device.id, e.message ?: "read failed")
            } finally {
                if (!closed) owner.emitDisconnected(device.id)
                runCatching { socket.close() }
            }
        }
    }

    suspend fun write(frame: ByteArray) = writeMutex.withLock {
        withContext(Dispatchers.IO) {
            val output: OutputStream = socket.outputStream
            try {
                output.write(frame)
                output.flush()
            } catch (e: IOException) {
                owner.emitError(device.id, e.message ?: "write failed")
                throw RuntimeException("send to ${device.id} failed: ${e.message}")
            }
        }
    }

    fun close() {
        if (closed) return
        closed = true
        readJob?.cancel()
        runCatching { socket.close() }
    }
}
