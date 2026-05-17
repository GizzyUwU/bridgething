package com.bridgething

import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.provider.Settings
import com.bridgething.session.BridgethingSessionBackend
import com.margelo.nitro.bridgething.session.BridgethingActiveWebapp
import com.margelo.nitro.bridgething.session.BridgethingAncsAuthStatus
import com.margelo.nitro.bridgething.session.BridgethingAncsSetupKind
import com.margelo.nitro.bridgething.session.BridgethingAncsSetupResult
import com.margelo.nitro.bridgething.session.BridgethingAuthState
import com.margelo.nitro.bridgething.session.BridgethingAuthKind
import com.margelo.nitro.bridgething.session.BridgethingBtBondState
import com.margelo.nitro.bridgething.session.BridgethingBtDevice
import com.margelo.nitro.bridgething.session.BridgethingBtDiscoveryEvent
import com.margelo.nitro.bridgething.session.BridgethingBtDiscoveryEventKind
import com.margelo.nitro.bridgething.session.BridgethingCapabilityFlags
import com.margelo.nitro.bridgething.session.BridgethingConfigEntry
import com.margelo.nitro.bridgething.session.BridgethingDeviceMeta
import com.margelo.nitro.bridgething.session.BridgethingHostInfo
import com.margelo.nitro.bridgething.session.BridgethingNowPlaying
import com.margelo.nitro.bridgething.session.BridgethingNowPlayingPlayback
import com.margelo.nitro.bridgething.session.BridgethingNowPlayingTrack
import com.margelo.nitro.bridgething.session.BridgethingOtaEvent
import com.margelo.nitro.bridgething.session.BridgethingOtaEventKind
import com.margelo.nitro.bridgething.session.BridgethingOtaKind
import com.margelo.nitro.bridgething.session.BridgethingOtaPhase
import com.margelo.nitro.bridgething.session.BridgethingOtaPollConfig
import com.margelo.nitro.bridgething.session.BridgethingProviderInfo
import com.margelo.nitro.bridgething.session.BridgethingRepeatMode
import com.margelo.nitro.bridgething.session.BridgethingSessionPeer
import com.margelo.nitro.bridgething.session.BridgethingWebappIcon
import com.margelo.nitro.bridgething.session.BridgethingWebappInfo
import dev.bridgething.companion.AncsSetupKind
import dev.bridgething.companion.BridgethingCompanion
import dev.bridgething.companion.BridgethingCompanionVersion
import dev.bridgething.companion.CompanionCapabilityFlags
import dev.bridgething.companion.CompanionLogLevel
import dev.bridgething.companion.HostInfo
import dev.bridgething.companion.OtaPhaseSnapshot
import dev.bridgething.companion.OtaPollConfig as KOtaPollConfig
import dev.bridgething.companion.OtaPollEvent
import dev.bridgething.gateway.BluetoothSocketAdapter
import dev.bridgething.gateway.GatewayEvent
import dev.bridgething.glue.BridgethingGlue
import dev.bridgething.glue.GlueNowPlaying
import dev.bridgething.lyrics.LrclibResolver
import dev.bridgething.lyrics.LyricsResolver
import dev.bridgething.schema.AncsAuthState
import dev.bridgething.schema.OtaKind
import dev.bridgething.schema.OtaPhase
import dev.bridgething.schema.RepeatMode
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeout

/**
 * Real [BridgethingSessionBackend] impl for the bridgething host app.
 * Owns one [BridgethingCompanion] (which owns the gateway, the active
 * glue, and every dispatcher).
 *
 * Mirror of the iOS `HybridBridgethingSessionImpl`. Glue registration
 * happens before the backend is installed: [BridgethingApp.installBridgething]
 * populates [registry] with one [ProviderRegistration] per provider id.
 *
 * Webapp install / uninstall / icon / config flows are intentionally
 * stubbed in this slice; they need additional gateway surface plumbing
 * (the iOS side is ~500 LOC of surface calls) and land in a follow-up.
 * The companion + dispatcher core is fully wired.
 */
public class HybridBridgethingSessionImpl(
    private val context: Context,
) : BridgethingSessionBackend {

    public data class ProviderRegistration(
        val id: String,
        val displayName: String,
        val available: Boolean,
        val factory: () -> BridgethingGlue,
        val signOut: () -> Unit,
    )

    public companion object {
        public var registry: List<ProviderRegistration> = emptyList()
        public var hostInfo: HostInfo = HostInfo(
            appName = "bridgething",
            appVersion = "0.0.0",
            osName = "Android",
        )
        public var lyricsResolver: LyricsResolver = LrclibResolver()

        // Class-of-device bridgething sets on the Car Thing for SDP
        // discovery. Mirror of `hciconfig hci0 class 0x7c0000` from the
        // yocto BSP. Internal to the impl - kept here because only one
        // companion object is allowed per class.
        internal const val BRIDGETHING_COD_CLASS = 0x7c0000

        internal const val EXTRA_REASON_KEY = "android.bluetooth.device.extra.REASON"
        internal const val EXTRA_REASON_UNKNOWN = -1

        // BluetoothDevice.UNBOND_REASON_* are @SystemApi but the integer
        // constants are stable. Map them to user-facing copy.
        internal fun describeUnbondReason(reason: Int): String = when (reason) {
            1 -> "the device's PIN didn't match"
            2 -> "the device rejected the pair request"
            3 -> "the system cancelled the pair"
            4 -> "the device didn't respond - make sure it's on, in range, and in pairing mode"
            5 -> "a scan was in progress; try again"
            6 -> "the pair timed out"
            7 -> "too many failed attempts; reboot the device or forget any stale bonds"
            8 -> "the device cancelled the pair on its end"
            9 -> "the bond was removed before it could finish"
            else -> "the device declined the bond (reason $reason)"
        }
    }

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val stateLock = Mutex()
    private var companion: BridgethingCompanion? = null
    private var btAdapter: BluetoothSocketAdapter? = null
    private var eventsJob: Job? = null
    private var otaJob: Job? = null
    private var authJob: Job? = null

    private val peers = ConcurrentHashMap<String, BridgethingSessionPeer>()

    @Volatile
    private var lastNowPlaying: BridgethingNowPlaying? = null

    @Volatile
    private var activeRegistration: ProviderRegistration? = null

    @Volatile
    private var onProviderChanged: ((BridgethingProviderInfo?) -> Unit)? = null

    @Volatile
    private var onAuthStateChanged: ((BridgethingAuthState) -> Unit)? = null

    @Volatile
    private var onPeerConnected: ((BridgethingSessionPeer) -> Unit)? = null

    @Volatile
    private var onPeerDisconnected: ((String) -> Unit)? = null

    @Volatile
    private var onNowPlayingChanged: ((BridgethingNowPlaying?) -> Unit)? = null

    @Volatile
    private var onAncsAuthStatusChanged: ((BridgethingAncsAuthStatus) -> Unit)? = null

    @Volatile
    private var onLog: ((String, String) -> Unit)? = null

    @Volatile
    private var onWebappsChanged: ((String) -> Unit)? = null

    @Volatile
    private var onDeviceMetaChanged: ((String, BridgethingDeviceMeta) -> Unit)? = null

    @Volatile
    private var onOtaEvent: ((BridgethingOtaEvent) -> Unit)? = null

    @Volatile
    private var onBtDiscoveryEvent: ((BridgethingBtDiscoveryEvent) -> Unit)? = null

    @Volatile
    private var onBtBondStateChanged: ((BridgethingBtDevice) -> Unit)? = null

    @Volatile
    private var logStreamingDesired: Boolean = false

    // Bluetooth discovery / pair state: receivers register lazily and stay
    // live across discovery/pair calls until [stop] tears them down.
    private val btMutex = Mutex()
    private var discoveryReceiver: BroadcastReceiver? = null
    private var bondReceiver: BroadcastReceiver? = null
    private val pendingBonds = ConcurrentHashMap<String, CompletableDeferred<BridgethingBtBondState>>()

    // ---- Lifecycle ----

    override suspend fun start() {
        stateLock.withLock {
            if (companion != null) return@withLock
            val adapter = BluetoothSocketAdapter()
            val host = makeHostInfo()
            val c = BridgethingCompanion(
                context = context.applicationContext,
                adapter = adapter,
                lyricsResolver = lyricsResolver,
                host = host,
                capabilities = CompanionCapabilityFlags(),
            )
            c.setNowPlayingObserver { np -> handleNowPlaying(np) }
            c.setAncsAuthStateObserver { state -> emitAncsAuthStatus(toRnAncsAuthStatus(state)) }
            if (logStreamingDesired) {
                c.setLogObserver { level, message -> onLog?.invoke(level.raw, message) }
            }
            c.start()

            eventsJob = scope.launch {
                c.gateway.events.collect { event -> handleGatewayEvent(event) }
            }
            otaJob = scope.launch {
                c.ota.events.collect { ev -> onOtaEvent?.invoke(toRnOtaEvent(ev)) }
            }
            companion = c
            btAdapter = adapter
            // Let BridgethingNotificationListener find the live gateway.
            NotificationBridgeRegistry.companion = c

            // Auto-connect to every bonded device that looks like a Car Thing.
            // Bond status is the user's gate (they pair once via system
            // settings); the adapter then opens an RFCOMM session per device.
            tryAutoConnect(adapter)
        }
    }

    override suspend fun stop() {
        var priorCompanion: BridgethingCompanion? = null
        var priorEvents: Job? = null
        var priorOta: Job? = null
        var priorAuth: Job? = null
        stateLock.withLock {
            priorCompanion = companion
            priorEvents = eventsJob
            priorOta = otaJob
            priorAuth = authJob
            companion = null
            btAdapter = null
            eventsJob = null
            otaJob = null
            authJob = null
        }
        priorEvents?.cancel()
        priorOta?.cancel()
        priorAuth?.cancel()
        try {
            priorCompanion?.stop()
        } finally {
            NotificationBridgeRegistry.companion = null
            unregisterBtReceivers()
            pendingBonds.values.forEach { it.complete(BridgethingBtBondState.NONE) }
            pendingBonds.clear()
            peers.clear()
            lastNowPlaying = null
            emitNowPlaying(null)
        }
    }

    private fun tryAutoConnect(adapter: BluetoothSocketAdapter) {
        // Host app must have requested BLUETOOTH_CONNECT before start();
        // SecurityException here means it didn't, so skip silently.
        val ba = bluetoothAdapter() ?: return
        val bonded = try { ba.bondedDevices } catch (_: SecurityException) { null } ?: return
        for (device in bonded) {
            if (!looksLikeCarThing(device)) continue
            scope.launch { runCatching { adapter.connect(device) } }
        }
    }

    private fun looksLikeCarThing(device: BluetoothDevice): Boolean {
        val cod = try { device.bluetoothClass?.hashCode() } catch (_: SecurityException) { null }
        if (cod == BRIDGETHING_COD_CLASS) return true
        val name = try { device.name } catch (_: SecurityException) { null } ?: return false
        return name.contains("Car Thing", ignoreCase = true) ||
            name.contains("bridgething", ignoreCase = true)
    }

    // ---- Provider selection ----

    override suspend fun availableProviders(): Array<BridgethingProviderInfo> = registry.map {
        BridgethingProviderInfo(id = it.id, displayName = it.displayName, available = it.available)
    }.toTypedArray()

    override suspend fun setActiveProvider(id: String?) {
        authJob?.cancel()
        val c = stateLock.withLock { companion } ?: error("session not started")
        if (id == null) {
            c.setActive(null)
            activeRegistration = null
            emitProvider(null)
            emitAuth(idleState())
            return
        }
        val registration = registry.firstOrNull { it.id == id }
            ?: error("unknown provider $id")
        activeRegistration = registration
        emitAuth(authState(BridgethingAuthKind.PENDING))
        try {
            val glue = registration.factory()
            c.setActive(glue)
            emitProvider(BridgethingProviderInfo(id = registration.id, displayName = registration.displayName, available = registration.available))
            emitAuth(authState(BridgethingAuthKind.AUTHENTICATED))
        } catch (e: Throwable) {
            emitAuth(authState(BridgethingAuthKind.FAILED, message = e.message ?: e.toString()))
            throw e
        }
    }

    override suspend fun cancelAuth() {
        authJob?.cancel()
        activeRegistration = null
        stateLock.withLock { companion }?.setActive(null)
        emitProvider(null)
        emitAuth(idleState())
    }

    override suspend fun signOut() {
        authJob?.cancel()
        val reg = activeRegistration
        activeRegistration = null
        runCatching { reg?.signOut?.invoke() }
        stateLock.withLock { companion }?.setActive(null)
        emitProvider(null)
        emitAuth(idleState())
    }

    override suspend fun currentProvider(): BridgethingProviderInfo? {
        val glue = stateLock.withLock { companion }?.current() ?: return null
        return registry.firstOrNull { it.id == glue.name }?.let {
            BridgethingProviderInfo(id = it.id, displayName = it.displayName, available = it.available)
        }
    }

    override suspend fun connectedPeers(): Array<BridgethingSessionPeer> = peers.values.toTypedArray()

    override suspend fun currentNowPlaying(): BridgethingNowPlaying? = lastNowPlaying

    // ---- ANCS ----

    override suspend fun enableAncsNotifications(): BridgethingAncsSetupResult {
        val result = stateLock.withLock { companion }?.enableAncsNotifications()
            ?: return BridgethingAncsSetupResult(
                kind = BridgethingAncsSetupKind.UNSUPPORTED,
                authStatus = BridgethingAncsAuthStatus.UNKNOWN,
                message = "session not started",
            )
        return BridgethingAncsSetupResult(
            kind = when (result.kind) {
                AncsSetupKind.Paired -> BridgethingAncsSetupKind.PAIRED
                AncsSetupKind.AlreadyPaired -> BridgethingAncsSetupKind.ALREADYPAIRED
                AncsSetupKind.Cancelled -> BridgethingAncsSetupKind.CANCELLED
                AncsSetupKind.Unsupported -> BridgethingAncsSetupKind.UNSUPPORTED
                AncsSetupKind.Failed -> BridgethingAncsSetupKind.FAILED
            },
            authStatus = toRnAncsAuthStatus(result.authState),
            message = result.message,
        )
    }

    override suspend fun ancsAuthStatus(): BridgethingAncsAuthStatus =
        toRnAncsAuthStatus(stateLock.withLock { companion }?.currentAncsAuthState() ?: AncsAuthState.Unknown)

    // ---- Webapps (per-device) - stubbed in this slice ----

    override suspend fun listWebapps(deviceId: String): Array<BridgethingWebappInfo> = TODO("android webapp install pipeline")
    override suspend fun currentWebapp(deviceId: String): BridgethingActiveWebapp? = null
    override suspend fun installWebappFromBase64(deviceId: String, archiveBase64: String): BridgethingWebappInfo = TODO("android webapp install pipeline")
    override suspend fun uninstallWebapp(deviceId: String, id: String): Unit = TODO("android webapp install pipeline")
    override suspend fun switchWebapp(deviceId: String, id: String): Unit = TODO("android webapp install pipeline")
    override suspend fun webappIcon(deviceId: String, id: String): BridgethingWebappIcon? = null
    override suspend fun listWebappConfig(deviceId: String, id: String): Array<BridgethingConfigEntry> = emptyArray()
    override suspend fun setWebappConfigField(deviceId: String, id: String, key: String, value: String): Unit = TODO("android webapp install pipeline")
    override suspend fun deleteWebappConfigField(deviceId: String, id: String, key: String): Unit = TODO("android webapp install pipeline")

    // ---- Capability flags ----

    override suspend fun setCapabilityFlags(flags: BridgethingCapabilityFlags) {
        stateLock.withLock { companion }?.setCapabilityFlags(
            CompanionCapabilityFlags(
                geo = flags.geo,
                notifications = flags.notifications,
                netFetch = flags.netFetch,
                netWs = flags.netWs,
                audioTts = flags.audioTts,
            )
        )
    }

    // ---- OTA ----

    override suspend fun setOtaPollConfig(config: BridgethingOtaPollConfig?) {
        val ota = stateLock.withLock { companion }?.ota ?: return
        if (config == null) {
            ota.setPollConfig(null)
        } else {
            ota.setPollConfig(
                KOtaPollConfig(
                    rootUrl = config.rootUrl ?: "https://ota.bridgething.com",
                    channel = config.channel,
                    intervalSeconds = config.intervalSeconds.toLong().coerceAtLeast(60L),
                    cacheDirectory = context.cacheDir,
                    autoPush = config.autoPush,
                )
            )
        }
    }

    override suspend fun pollOtaNow() {
        stateLock.withLock { companion }?.ota?.pollNow()
    }

    override suspend fun deviceMeta(deviceId: String): BridgethingDeviceMeta? {
        val meta = stateLock.withLock { companion }?.ota?.meta(deviceId) ?: return null
        return BridgethingDeviceMeta(
            daemonVersion = meta.appVersion,
            appName = meta.appName,
            osName = meta.osName,
            osVersion = meta.osVersion,
            channel = meta.channel,
            modelName = meta.modelName,
            serialNumber = meta.serialNumber,
        )
    }

    // ---- Host identity ----

    override suspend fun hostInfo(): BridgethingHostInfo {
        val host = makeHostInfo()
        return BridgethingHostInfo(
            appName = host.appName,
            appVersion = host.appVersion,
            osName = host.osName,
            osVersion = host.osVersion,
            hostIdentifier = host.address,
            libVersion = BridgethingCompanionVersion.LIB,
            libbridgethingVersion = BridgethingCompanionVersion.LIBBRIDGETHING,
            adapterVersion = host.adapterVersion,
        )
    }

    @Suppress("HardwareIds")
    private fun makeHostInfo(): HostInfo = HostInfo(
        appName = hostInfo.appName,
        appVersion = hostInfo.appVersion,
        osName = "Android",
        osVersion = android.os.Build.VERSION.RELEASE ?: "",
        address = Settings.Secure.getString(context.contentResolver, Settings.Secure.ANDROID_ID) ?: "",
        adapterVersion = "rfcomm",
    )

    // ---- Notification access ----

    override suspend fun isNotificationAccessGranted(): Boolean {
        val ctx = context.applicationContext
        val packages = androidx.core.app.NotificationManagerCompat
            .getEnabledListenerPackages(ctx)
        return packages.contains(ctx.packageName)
    }

    override suspend fun requestNotificationAccess() {
        val ctx = context.applicationContext
        val intent = Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS)
            .apply { addFlags(Intent.FLAG_ACTIVITY_NEW_TASK) }
        ctx.startActivity(intent)
    }

    override suspend fun revokeRuntimePermissions(permissions: Array<String>): Boolean {
        if (android.os.Build.VERSION.SDK_INT < android.os.Build.VERSION_CODES.TIRAMISU) {
            return false
        }
        if (permissions.isEmpty()) return false
        return try {
            context.applicationContext.revokeSelfPermissionsOnKill(permissions.toList())
            true
        } catch (e: Throwable) {
            android.util.Log.w(
                "bridgething.session",
                "revokeSelfPermissionsOnKill threw: ${e.message}",
            )
            false
        }
    }

    override suspend fun killApp() {
        // finishAffinity() doesn't drop the pid, so the queued
        // revokeSelfPermissionsOnKill grants would never apply.
        android.os.Process.killProcess(android.os.Process.myPid())
    }

    // ---- In-app Bluetooth pairing ----

    override suspend fun listBondedBluetoothDevices(): Array<BridgethingBtDevice> {
        val adapter = bluetoothAdapter() ?: return emptyArray()
        return try {
            adapter.bondedDevices.map(::toBtDevice).toTypedArray()
        } catch (_: SecurityException) {
            emptyArray()
        }
    }

    override suspend fun startBluetoothDiscovery() {
        val adapter = bluetoothAdapter() ?: error("bluetooth unavailable on this device")
        btMutex.withLock {
            ensureDiscoveryReceiver()
            try {
                if (adapter.isDiscovering) adapter.cancelDiscovery()
                val ok = adapter.startDiscovery()
                if (!ok) {
                    onBtDiscoveryEvent?.invoke(
                        makeBtDiscoveryEvent(BridgethingBtDiscoveryEventKind.FAILED, reason = "BluetoothAdapter.startDiscovery() returned false (missing perm, BT off, or already discovering)")
                    )
                }
            } catch (e: SecurityException) {
                onBtDiscoveryEvent?.invoke(
                    makeBtDiscoveryEvent(BridgethingBtDiscoveryEventKind.FAILED, reason = e.message ?: "BLUETOOTH_SCAN denied")
                )
            }
        }
    }

    override suspend fun stopBluetoothDiscovery() {
        val adapter = bluetoothAdapter() ?: return
        try {
            if (adapter.isDiscovering) adapter.cancelDiscovery()
        } catch (_: SecurityException) {
            // Caller already lost permission; the receiver's FINISHED event
            // will fire (or already did) so nothing else to do.
        }
    }

    override suspend fun presentPairPicker(): BridgethingBtDevice? {
        // android's happy-path is the in-app BluetoothPairPicker
        // component; this method is the iOS AccessorySetupKit entrypoint.
        error("presentPairPicker is iOS-only; use the BluetoothPairPicker component on android")
    }

    override suspend fun pairBluetoothDevice(address: String): BridgethingBtBondState {
        val adapter = bluetoothAdapter() ?: error("bluetooth unavailable on this device")
        val device = try {
            adapter.getRemoteDevice(address)
        } catch (e: IllegalArgumentException) {
            error("invalid bluetooth address: ${e.message}")
        }
        ensureBondReceiver()
        if (currentBondState(device) == BluetoothDevice.BOND_BONDED) {
            scope.launch { runCatching { connectIfBridgething(device) } }
            return BridgethingBtBondState.BONDED
        }
        // Android refuses createBond() while inquiry is running.
        try {
            if (adapter.isDiscovering) adapter.cancelDiscovery()
        } catch (_: SecurityException) {}

        val deferred = CompletableDeferred<BridgethingBtBondState>()
        pendingBonds[address] = deferred
        val started = try {
            device.createBond()
        } catch (e: SecurityException) {
            pendingBonds.remove(address)
            error("BLUETOOTH_CONNECT denied: ${e.message}")
        }
        if (!started) {
            pendingBonds.remove(address)
            error("createBond() returned false (already bonding, or unsupported)")
        }
        return try {
            withTimeout(60_000L) { deferred.await() }
        } catch (_: TimeoutCancellationException) {
            pendingBonds.remove(address)
            error("pair timed out after 60s")
        } catch (e: PairException) {
            error(e.message ?: "pair failed")
        }
    }

    /** Exception carrying a friendly unbond reason from the bond receiver. */
    private class PairException(message: String) : RuntimeException(message)

    private fun bluetoothAdapter(): BluetoothAdapter? {
        val ctx = context.applicationContext
        return (ctx.getSystemService(Context.BLUETOOTH_SERVICE) as? BluetoothManager)?.adapter
    }

    private fun currentBondState(device: BluetoothDevice): Int = try {
        device.bondState
    } catch (_: SecurityException) {
        BluetoothDevice.BOND_NONE
    }

    private fun ensureDiscoveryReceiver() {
        if (discoveryReceiver != null) return
        val ctx = context.applicationContext
        val recv = object : BroadcastReceiver() {
            override fun onReceive(context: Context?, intent: Intent?) {
                val action = intent?.action ?: return
                // Hop off the main thread before any binder lookups
                // (toBtDevice does 3) or JS bridge calls.
                scope.launch { dispatchDiscoveryEvent(action, intent) }
            }
        }
        val filter = IntentFilter().apply {
            addAction(BluetoothAdapter.ACTION_DISCOVERY_STARTED)
            addAction(BluetoothAdapter.ACTION_DISCOVERY_FINISHED)
            addAction(BluetoothDevice.ACTION_FOUND)
        }
        registerSystemReceiver(ctx, recv, filter)
        discoveryReceiver = recv
    }

    private fun dispatchDiscoveryEvent(action: String, intent: Intent) {
        when (action) {
            BluetoothAdapter.ACTION_DISCOVERY_STARTED ->
                onBtDiscoveryEvent?.invoke(makeBtDiscoveryEvent(BridgethingBtDiscoveryEventKind.STARTED))
            BluetoothAdapter.ACTION_DISCOVERY_FINISHED ->
                onBtDiscoveryEvent?.invoke(makeBtDiscoveryEvent(BridgethingBtDiscoveryEventKind.FINISHED))
            BluetoothDevice.ACTION_FOUND -> {
                val device = extractBtDevice(intent) ?: return
                onBtDiscoveryEvent?.invoke(
                    makeBtDiscoveryEvent(BridgethingBtDiscoveryEventKind.FOUND, device = toBtDevice(device))
                )
            }
        }
    }

    private suspend fun ensureBondReceiver() {
        if (bondReceiver != null) return
        btMutex.withLock {
            if (bondReceiver != null) return@withLock
            val ctx = context.applicationContext
            val recv = object : BroadcastReceiver() {
                override fun onReceive(context: Context?, intent: Intent?) {
                    if (intent?.action != BluetoothDevice.ACTION_BOND_STATE_CHANGED) return
                    scope.launch { dispatchBondStateChange(intent) }
                }
            }
            registerSystemReceiver(ctx, recv, IntentFilter(BluetoothDevice.ACTION_BOND_STATE_CHANGED))
            bondReceiver = recv
        }
    }

    private fun dispatchBondStateChange(intent: Intent) {
        val device = extractBtDevice(intent) ?: return
        val state = intent.getIntExtra(BluetoothDevice.EXTRA_BOND_STATE, BluetoothDevice.ERROR)
        onBtBondStateChanged?.invoke(toBtDevice(device))

        val address = device.address ?: return
        val deferred = pendingBonds[address] ?: return
        when (state) {
            BluetoothDevice.BOND_BONDED -> {
                pendingBonds.remove(address)
                deferred.complete(BridgethingBtBondState.BONDED)
                scope.launch { runCatching { connectIfBridgething(device) } }
            }
            BluetoothDevice.BOND_NONE -> {
                pendingBonds.remove(address)
                val reason = intent.getIntExtra(EXTRA_REASON_KEY, EXTRA_REASON_UNKNOWN)
                deferred.completeExceptionally(PairException(describeUnbondReason(reason)))
            }
        }
    }

    private fun extractBtDevice(intent: Intent): BluetoothDevice? =
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.TIRAMISU) {
            intent.getParcelableExtra(BluetoothDevice.EXTRA_DEVICE, BluetoothDevice::class.java)
        } else {
            @Suppress("DEPRECATION") intent.getParcelableExtra(BluetoothDevice.EXTRA_DEVICE)
        }

    private fun registerSystemReceiver(ctx: Context, receiver: BroadcastReceiver, filter: IntentFilter) {
        // System broadcasts on Tiramisu+ need RECEIVER_EXPORTED.
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.TIRAMISU) {
            ctx.registerReceiver(receiver, filter, Context.RECEIVER_EXPORTED)
        } else {
            @Suppress("UnspecifiedRegisterReceiverFlag")
            ctx.registerReceiver(receiver, filter)
        }
    }

    private suspend fun connectIfBridgething(device: BluetoothDevice) {
        // Open an RFCOMM session over the newly-bonded device using the
        // live adapter the companion is sharing. Drops silently if the
        // companion isn't running (no peer for the gateway to attach to).
        val adapter = stateLock.withLock { btAdapter } ?: return
        runCatching { adapter.connect(device) }
    }

    private fun toBtDevice(device: BluetoothDevice): BridgethingBtDevice {
        val name = try { device.name } catch (_: SecurityException) { null }
        // BluetoothClass's hashCode is the raw 24-bit CoD per AOSP source.
        // The Car Thing advertises 0x7c0000 (set by the yocto-superbird
        // image via `hciconfig hci0 class 0x7c0000`). Name fallback covers
        // pre-bond detection where the class may not have been read yet.
        val cod = try { device.bluetoothClass?.hashCode() } catch (_: SecurityException) { null }
        val isCarThing =
            cod == BRIDGETHING_COD_CLASS ||
                (name?.contains("Car Thing", ignoreCase = true) == true) ||
                (name?.contains("bridgething", ignoreCase = true) == true)
        return BridgethingBtDevice(
            address = device.address ?: "",
            name = name,
            bondState = mapBondState(currentBondState(device)),
            isCarThing = isCarThing,
        )
    }


    private fun mapBondState(state: Int): BridgethingBtBondState = when (state) {
        BluetoothDevice.BOND_BONDED -> BridgethingBtBondState.BONDED
        BluetoothDevice.BOND_BONDING -> BridgethingBtBondState.BONDING
        else -> BridgethingBtBondState.NONE
    }

    private fun makeBtDiscoveryEvent(
        kind: BridgethingBtDiscoveryEventKind,
        device: BridgethingBtDevice? = null,
        reason: String? = null,
    ): BridgethingBtDiscoveryEvent = BridgethingBtDiscoveryEvent(
        kind = kind,
        device = device,
        reason = reason,
    )

    private fun unregisterBtReceivers() {
        val ctx = context.applicationContext
        discoveryReceiver?.let { runCatching { ctx.unregisterReceiver(it) } }
        bondReceiver?.let { runCatching { ctx.unregisterReceiver(it) } }
        discoveryReceiver = null
        bondReceiver = null
    }

    // ---- Callback setters ----

    override fun setOnProviderChanged(callback: (BridgethingProviderInfo?) -> Unit) { onProviderChanged = callback }
    override fun setOnAuthStateChanged(callback: (BridgethingAuthState) -> Unit) { onAuthStateChanged = callback }
    override fun setOnPeerConnected(callback: (BridgethingSessionPeer) -> Unit) { onPeerConnected = callback }
    override fun setOnPeerDisconnected(callback: (String) -> Unit) { onPeerDisconnected = callback }
    override fun setOnNowPlayingChanged(callback: (BridgethingNowPlaying?) -> Unit) { onNowPlayingChanged = callback }
    override fun setOnAncsAuthStatusChanged(callback: (BridgethingAncsAuthStatus) -> Unit) { onAncsAuthStatusChanged = callback }
    override fun setOnLog(callback: (String, String) -> Unit) { onLog = callback }

    override fun setLogStreamingEnabled(enabled: Boolean) {
        logStreamingDesired = enabled
        val c = companion ?: return
        if (enabled) {
            c.setLogObserver { level, msg -> onLog?.invoke(level.raw, msg) }
        } else {
            c.setLogObserver(null)
        }
    }

    override fun setOnWebappsChanged(callback: (String) -> Unit) { onWebappsChanged = callback }
    override fun setOnDeviceMetaChanged(callback: (String, BridgethingDeviceMeta) -> Unit) { onDeviceMetaChanged = callback }
    override fun setOnOtaEvent(callback: (BridgethingOtaEvent) -> Unit) { onOtaEvent = callback }
    override fun setOnBluetoothDiscoveryEvent(callback: (BridgethingBtDiscoveryEvent) -> Unit) { onBtDiscoveryEvent = callback }
    override fun setOnBluetoothBondStateChanged(callback: (BridgethingBtDevice) -> Unit) { onBtBondStateChanged = callback }

    // ---- Internal ----

    private fun handleGatewayEvent(event: GatewayEvent) {
        when (event) {
            is GatewayEvent.Connected -> {
                val peer = BridgethingSessionPeer(id = event.device.id, name = event.device.name)
                peers[event.device.id] = peer
                onPeerConnected?.invoke(peer)
            }
            is GatewayEvent.Disconnected -> {
                peers.remove(event.deviceId)
                onPeerDisconnected?.invoke(event.deviceId)
            }
            is GatewayEvent.Message -> Unit
            is GatewayEvent.DecodeError -> Unit
        }
    }

    private fun handleNowPlaying(np: GlueNowPlaying?) {
        val mapped = np?.let { snapshot ->
            val update = snapshot.update
            BridgethingNowPlaying(
                track = update.mediaItem?.let { t ->
                    BridgethingNowPlayingTrack(
                        id = t.persistentId,
                        title = t.title,
                        artist = t.artist,
                        album = t.album,
                        artworkUrl = snapshot.artworkUrl,
                        durationMs = t.durationMs?.toDouble(),
                    )
                },
                playback = BridgethingNowPlayingPlayback(
                    playing = update.playback?.playing ?: false,
                    positionMs = (update.playback?.positionMs ?: 0u).toDouble(),
                    shuffle = update.playback?.shuffle ?: false,
                    repeatMode = toRnRepeatMode(update.playback?.repeat ?: RepeatMode.Off),
                ),
                appName = update.playback?.appDisplayName,
            )
        }
        // Glues tick handleNowPlaying once a second on the position
        // update; skip the JS bridge hop when nothing visible changed.
        if (mapped == lastNowPlaying) return
        lastNowPlaying = mapped
        emitNowPlaying(mapped)
    }

    private fun emitProvider(info: BridgethingProviderInfo?) { onProviderChanged?.invoke(info) }
    private fun emitAuth(state: BridgethingAuthState) { onAuthStateChanged?.invoke(state) }
    private fun emitNowPlaying(np: BridgethingNowPlaying?) { onNowPlayingChanged?.invoke(np) }
    private fun emitAncsAuthStatus(status: BridgethingAncsAuthStatus) { onAncsAuthStatusChanged?.invoke(status) }

    private fun idleState() = authState(BridgethingAuthKind.IDLE)

    private fun authState(
        kind: BridgethingAuthKind,
        message: String? = null,
    ): BridgethingAuthState = BridgethingAuthState(
        kind = kind,
        userCode = null,
        verificationUrl = null,
        verificationUrlComplete = null,
        message = message,
    )

    private fun toRnAncsAuthStatus(state: AncsAuthState): BridgethingAncsAuthStatus = when (state) {
        AncsAuthState.Unknown -> BridgethingAncsAuthStatus.UNKNOWN
        AncsAuthState.Probing -> BridgethingAncsAuthStatus.PROBING
        AncsAuthState.Authorized -> BridgethingAncsAuthStatus.AUTHORIZED
        AncsAuthState.Unauthorized -> BridgethingAncsAuthStatus.UNAUTHORIZED
    }

    private fun toRnRepeatMode(mode: RepeatMode): BridgethingRepeatMode = when (mode) {
        RepeatMode.Off -> BridgethingRepeatMode.OFF
        RepeatMode.One -> BridgethingRepeatMode.ONE
        RepeatMode.All -> BridgethingRepeatMode.ALL
    }

    private fun toRnOtaEvent(ev: OtaPollEvent): BridgethingOtaEvent = when (ev) {
        is OtaPollEvent.ManifestPolled -> makeOtaEvent(
            kind = BridgethingOtaEventKind.MANIFESTPOLLED, updatedAt = ev.updatedAt,
        )
        is OtaPollEvent.ManifestPollFailed -> makeOtaEvent(
            kind = BridgethingOtaEventKind.MANIFESTPOLLFAILED, reason = ev.reason,
        )
        is OtaPollEvent.ChannelMismatch -> makeOtaEvent(
            kind = BridgethingOtaEventKind.CHANNELMISMATCH,
            deviceId = ev.deviceId, deviceChannel = ev.deviceChannel, configuredChannel = ev.configuredChannel,
        )
        is OtaPollEvent.UpdateAvailable -> makeOtaEvent(
            kind = BridgethingOtaEventKind.UPDATEAVAILABLE,
            deviceId = ev.deviceId, otaKind = toRnOtaKind(ev.kind),
            fromVersion = ev.fromVersion, toVersion = ev.toVersion,
        )
        is OtaPollEvent.Progress -> {
            val (phase, percent) = unwrapSnapshot(ev.snapshot)
            makeOtaEvent(
                kind = BridgethingOtaEventKind.PROGRESS,
                deviceId = ev.deviceId, otaKind = toRnOtaKind(ev.kind),
                phase = phase, percent = percent?.toDouble(),
            )
        }
        is OtaPollEvent.Updated -> makeOtaEvent(
            kind = BridgethingOtaEventKind.UPDATED,
            deviceId = ev.deviceId, otaKind = toRnOtaKind(ev.kind), toVersion = ev.version,
        )
        is OtaPollEvent.Failed -> makeOtaEvent(
            kind = BridgethingOtaEventKind.FAILED,
            deviceId = ev.deviceId, otaKind = toRnOtaKind(ev.kind), reason = ev.reason,
        )
    }

    private fun unwrapSnapshot(snapshot: OtaPhaseSnapshot): Pair<BridgethingOtaPhase?, Int?> = when (snapshot) {
        OtaPhaseSnapshot.Idle -> Pair(BridgethingOtaPhase.IDLE, null)
        is OtaPhaseSnapshot.Streaming -> Pair(BridgethingOtaPhase.STREAMING, snapshot.percent)
        is OtaPhaseSnapshot.Applying -> {
            val rnPhase = when (snapshot.phase) {
                OtaPhase.Streaming -> BridgethingOtaPhase.STREAMING
                OtaPhase.Verifying -> BridgethingOtaPhase.VERIFYING
                OtaPhase.Writing -> BridgethingOtaPhase.WRITING
                OtaPhase.Confirming -> BridgethingOtaPhase.CONFIRMING
                OtaPhase.Reboot -> BridgethingOtaPhase.REBOOT
            }
            Pair(rnPhase, snapshot.percent)
        }
        OtaPhaseSnapshot.Completed -> Pair(BridgethingOtaPhase.COMPLETED, null)
        is OtaPhaseSnapshot.Failed -> Pair(BridgethingOtaPhase.FAILED, null)
    }

    private fun toRnOtaKind(kind: OtaKind): BridgethingOtaKind = when (kind) {
        OtaKind.Image -> BridgethingOtaKind.IMAGE
        OtaKind.Daemon -> BridgethingOtaKind.DAEMON
    }

    private fun makeOtaEvent(
        kind: BridgethingOtaEventKind,
        updatedAt: String? = null,
        reason: String? = null,
        deviceId: String? = null,
        otaKind: BridgethingOtaKind? = null,
        fromVersion: String? = null,
        toVersion: String? = null,
        phase: BridgethingOtaPhase? = null,
        percent: Double? = null,
        deviceChannel: String? = null,
        configuredChannel: String? = null,
    ): BridgethingOtaEvent = BridgethingOtaEvent(
        kind = kind,
        updatedAt = updatedAt,
        reason = reason,
        deviceId = deviceId,
        otaKind = otaKind,
        fromVersion = fromVersion,
        toVersion = toVersion,
        phase = phase,
        percent = percent,
        deviceChannel = deviceChannel,
        configuredChannel = configuredChannel,
    )
}
