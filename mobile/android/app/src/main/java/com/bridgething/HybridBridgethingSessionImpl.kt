package com.bridgething

import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothManager
import android.content.Context
import android.content.Intent
import android.provider.Settings
import com.bridgething.session.BridgethingSessionBackend
import com.margelo.nitro.bridgething.session.BridgethingActiveWebapp
import com.margelo.nitro.bridgething.session.BridgethingAncsAuthStatus
import com.margelo.nitro.bridgething.session.BridgethingAncsSetupKind
import com.margelo.nitro.bridgething.session.BridgethingAncsSetupResult
import com.margelo.nitro.bridgething.session.BridgethingAuthState
import com.margelo.nitro.bridgething.session.BridgethingAuthKind
import com.margelo.nitro.bridgething.session.BridgethingBtDevice
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
import dev.bridgething.companion.AndroidNotificationActionBackend
import dev.bridgething.companion.AndroidPhoneBackend
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
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/** [BridgethingSessionBackend] impl that owns one [BridgethingCompanion]. */
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

        private const val REQUEST_DIALER_ROLE = 0xBA02
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
    private var logStreamingDesired: Boolean = false

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
                notificationActions = AndroidNotificationActionBackend { id, positive ->
                    NotificationBridgeRegistry.listener?.actionIntent(id, positive)
                },
                phone = AndroidPhoneBackend(context.applicationContext),
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
            NotificationBridgeRegistry.companion = c

            // reopen RFCOMM for every device already authorized in CDM's association list.
            reconnectAssociated(adapter)
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
            peers.clear()
            lastNowPlaying = null
            emitNowPlaying(null)
        }
    }

    private fun reconnectAssociated(adapter: BluetoothSocketAdapter) {
        val ba = bluetoothAdapter() ?: return
        for (mac in CompanionDevicePicker.associations(context.applicationContext)) {
            val device = runCatching { ba.getRemoteDevice(mac) }.getOrNull() ?: continue
            scope.launch { runCatching { adapter.connect(device) } }
        }
    }

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

    override suspend fun listWebapps(deviceId: String): Array<BridgethingWebappInfo> = TODO("android webapp install pipeline")
    override suspend fun currentWebapp(deviceId: String): BridgethingActiveWebapp? = null
    override suspend fun installWebappFromBase64(deviceId: String, archiveBase64: String): BridgethingWebappInfo = TODO("android webapp install pipeline")
    override suspend fun uninstallWebapp(deviceId: String, id: String): Unit = TODO("android webapp install pipeline")
    override suspend fun switchWebapp(deviceId: String, id: String): Unit = TODO("android webapp install pipeline")
    override suspend fun webappIcon(deviceId: String, id: String): BridgethingWebappIcon? = null
    override suspend fun listWebappConfig(deviceId: String, id: String): Array<BridgethingConfigEntry> = emptyArray()
    override suspend fun setWebappConfigField(deviceId: String, id: String, key: String, value: String): Unit = TODO("android webapp install pipeline")
    override suspend fun deleteWebappConfigField(deviceId: String, id: String, key: String): Unit = TODO("android webapp install pipeline")

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

    override suspend fun isDefaultDialer(): Boolean {
        val ctx = context.applicationContext
        return if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.Q) {
            ctx.getSystemService(android.app.role.RoleManager::class.java)
                ?.isRoleHeld(android.app.role.RoleManager.ROLE_DIALER) == true
        } else {
            val telecom = ctx.getSystemService(Context.TELECOM_SERVICE) as? android.telecom.TelecomManager
            telecom?.defaultDialerPackage == ctx.packageName
        }
    }

    override suspend fun requestDefaultDialer() {
        val ctx = context.applicationContext
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.Q) {
            val roleManager = ctx.getSystemService(android.app.role.RoleManager::class.java) ?: return
            if (!roleManager.isRoleAvailable(android.app.role.RoleManager.ROLE_DIALER)) return
            // RoleManager's request dialog is an activity-result api; await its close so the
            // caller can re-check isDefaultDialer.
            val activity = BridgethingActivityRegistry.currentActivity ?: return
            val intent = roleManager.createRequestRoleIntent(android.app.role.RoleManager.ROLE_DIALER)
            val done = CompletableDeferred<Unit>()
            BridgethingActivityRegistry.expectResult(REQUEST_DIALER_ROLE) { _, _ -> done.complete(Unit) }
            activity.startActivityForResult(intent, REQUEST_DIALER_ROLE)
            done.await()
        } else {
            @Suppress("DEPRECATION")
            val intent = Intent(android.telecom.TelecomManager.ACTION_CHANGE_DEFAULT_DIALER)
                .putExtra(android.telecom.TelecomManager.EXTRA_CHANGE_DEFAULT_DIALER_PACKAGE_NAME, ctx.packageName)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            ctx.startActivity(intent)
        }
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
        // finishAffinity doesn't drop the pid so the queued revoke never applies.
        android.os.Process.killProcess(android.os.Process.myPid())
    }

    override suspend fun presentPairPicker(): BridgethingBtDevice? {
        val picked = CompanionDevicePicker.pick(context.applicationContext) ?: return null
        // connect immediately so the new peer appears without an app restart.
        val adapter = stateLock.withLock { btAdapter } ?: return picked
        scope.launch { reconnectAssociated(adapter) }
        return picked
    }

    private fun bluetoothAdapter(): BluetoothAdapter? {
        val ctx = context.applicationContext
        return (ctx.getSystemService(Context.BLUETOOTH_SERVICE) as? BluetoothManager)?.adapter
    }

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
        // glues tick once a second on position; skip the JS bridge hop when nothing visible changed.
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
        OtaKind.BuiltinWebapp -> BridgethingOtaKind.BUILTINWEBAPP
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
