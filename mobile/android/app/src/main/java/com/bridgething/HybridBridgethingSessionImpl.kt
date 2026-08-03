package com.bridgething

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.Settings
import com.bridgething.session.BridgethingSessionBackend
import com.margelo.nitro.bridgething.session.BridgethingActiveWebapp
import com.margelo.nitro.bridgething.session.BridgethingAncsAuthStatus
import com.margelo.nitro.bridgething.session.BridgethingAncsAuthStatusEntry
import com.margelo.nitro.bridgething.session.BridgethingAncsSetupKind
import com.margelo.nitro.bridgething.session.BridgethingAncsSetupResult
import com.margelo.nitro.bridgething.session.BridgethingAuthState
import com.margelo.nitro.bridgething.session.BridgethingAuthKind
import com.margelo.nitro.bridgething.session.BridgethingBtBondState
import com.margelo.nitro.bridgething.session.BridgethingBtDevice
import com.margelo.nitro.bridgething.session.BridgethingCapabilityFlags
import com.margelo.nitro.bridgething.session.BridgethingCompanionDebug
import com.margelo.nitro.bridgething.session.BridgethingConfigEntry
import com.margelo.nitro.bridgething.session.BridgethingConfigField
import com.margelo.nitro.bridgething.session.BridgethingConfigKind
import com.margelo.nitro.bridgething.session.BridgethingDeviceMeta
import com.margelo.nitro.bridgething.session.BridgethingDocEntry
import com.margelo.nitro.bridgething.session.BridgethingDeviceMetaEntry
import com.margelo.nitro.bridgething.session.BridgethingDeviceLogLine
import com.margelo.nitro.bridgething.session.BridgethingHostInfo
import com.margelo.nitro.bridgething.session.BridgethingLogArchive
import com.margelo.nitro.bridgething.session.BridgethingSessionSnapshot
import com.margelo.nitro.bridgething.session.BridgethingNowPlaying
import com.margelo.nitro.bridgething.session.BridgethingNowPlayingPlayback
import com.margelo.nitro.bridgething.session.BridgethingNowPlayingTrack
import com.margelo.nitro.bridgething.session.BridgethingOtaChannelInfo
import com.margelo.nitro.bridgething.session.BridgethingOtaKind
import com.margelo.nitro.bridgething.session.BridgethingOtaManifest
import com.margelo.nitro.bridgething.session.BridgethingOtaPhase
import com.margelo.nitro.bridgething.session.BridgethingOtaRun
import com.margelo.nitro.bridgething.session.BridgethingOtaPollStatus
import com.margelo.nitro.bridgething.session.BridgethingOtaOutcome
import com.margelo.nitro.bridgething.session.BridgethingOtaAvailable
import com.margelo.nitro.bridgething.session.BridgethingDeviceWebappsEntry
import com.margelo.nitro.bridgething.session.BridgethingOtaPollConfig
import com.margelo.nitro.bridgething.session.BridgethingOtaRelease
import com.margelo.nitro.bridgething.session.BridgethingOtaStep
import com.margelo.nitro.bridgething.session.BridgethingOtaStepKind
import com.margelo.nitro.bridgething.session.BridgethingPeerLinkStatus
import com.margelo.nitro.bridgething.session.BridgethingProviderInfo
import com.margelo.nitro.bridgething.session.BridgethingRepeatMode
import com.margelo.nitro.bridgething.session.BridgethingServiceHealth
import com.margelo.nitro.bridgething.session.BridgethingServiceHealthKind
import com.margelo.nitro.bridgething.session.BridgethingSessionPeer
import com.margelo.nitro.bridgething.session.BridgethingVoiceModelState
import com.margelo.nitro.bridgething.session.BridgethingVoiceModelStatus
import com.margelo.nitro.bridgething.session.BridgethingWebappIcon
import com.margelo.nitro.bridgething.session.BridgethingWebappInfo
import com.margelo.nitro.bridgething.session.BridgethingWebappSlot
import com.margelo.nitro.bridgething.session.BridgethingWebappSlots
import com.margelo.nitro.bridgething.session.BridgethingWebappRole
import com.margelo.nitro.bridgething.session.BridgethingWebappSource
import com.bridgething.companion.AncsSetupKind
import com.bridgething.companion.BridgethingCompanion
import com.bridgething.companion.BridgethingCompanionVersion
import com.bridgething.companion.CompanionCapabilityFlags
import com.bridgething.companion.CompanionLogLevel
import com.bridgething.companion.DeviceLogRing
import com.bridgething.companion.HostInfo
import com.bridgething.companion.ModelBundleState
import com.bridgething.companion.OtaCompositeVersion
import com.bridgething.companion.OtaDiscoverManifest
import com.bridgething.companion.OtaPhaseSnapshot
import com.bridgething.companion.OtaPollConfig as KOtaPollConfig
import com.bridgething.companion.OtaPollEvent
import com.bridgething.companion.OtaStoreChange
import com.bridgething.companion.OtaRunPhase
import com.bridgething.companion.OtaRunOutcome
import com.bridgething.companion.OtaRun
import com.bridgething.companion.OtaPollStatus
import com.bridgething.companion.OtaAvailable
import com.bridgething.companion.OtaStepKind
import com.bridgething.companion.WebappInstallResult
import com.bridgething.gateway.GatewayEvent
import com.bridgething.gateway.LogStore
import com.bridgething.gateway.RequestResult
import com.bridgething.gateway.device
import com.bridgething.gateway.system
import com.bridgething.gateway.webapp
import com.bridgething.glue.BridgethingGlue
import com.bridgething.glue.GlueAuthState
import com.bridgething.glue.GlueDebugState
import com.bridgething.glue.GlueNowPlaying
import com.bridgething.schema.BridgeThingMeta
import com.bridgething.schema.BridgeToGatewayMsgData
import com.bridgething.glue.GlueServiceHealth
import com.bridgething.lyrics.LrclibResolver
import com.bridgething.lyrics.LyricsResolver
import com.bridgething.schema.AncsAuthState
import com.bridgething.schema.ConfigField
import com.bridgething.schema.DeviceSetNickname
import com.bridgething.schema.OtaKind
import com.bridgething.schema.OtaPhase
import com.bridgething.schema.Priority
import com.bridgething.schema.RepeatMode
import com.bridgething.schema.WebappConfigDelete
import com.bridgething.schema.WebappConfigList
import com.bridgething.schema.WebappConfigSet
import com.bridgething.schema.WebappError
import com.bridgething.schema.WebappDocDelete
import com.bridgething.schema.WebappDocGet
import com.bridgething.schema.WebappDocList
import com.bridgething.schema.WebappDocSet
import com.bridgething.schema.WebappResourceKind
import com.bridgething.schema.WebappInfo
import com.bridgething.schema.WebappRole
import com.bridgething.schema.WebappSource
import com.bridgething.schema.WebappSetSlot
import com.bridgething.schema.WebappSlot
import com.bridgething.schema.WebappSlots
import com.bridgething.schema.WebappSwitchTo
import com.bridgething.schema.WebappUninstall
import java.io.File
import java.net.HttpURLConnection
import java.net.URI
import java.net.URL
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.Json

public class HybridBridgethingSessionImpl(
    private val context: Context,
) : BridgethingSessionBackend {

    public data class ProviderRegistration(
        val id: String,
        val displayName: String,
        val available: Boolean,
        val factory: () -> BridgethingGlue,
        val signOut: () -> Unit,
        val hasCredentials: () -> Boolean = { false },
    )

    public companion object {
        public var registry: List<ProviderRegistration> = emptyList()
        public var hostInfo: HostInfo = HostInfo(
            appName = "bridgething",
            appVersion = "0.0.0",
            osName = "Android",
        )
        public var lyricsResolver: LyricsResolver = LrclibResolver()

        private const val PREFS_NAME = "bridgething.session"
        private const val VOICE_MODEL_KEY = "caps.voiceModel"
        private const val WEBAPPS_READ_ATTEMPTS = 3
        private const val REQUEST_DIALER_ROLE = 0xBA02
        private const val AUTO_RESUME_PREFIX = "autoresume."
        private const val CONNECTED_PROVIDERS_KEY = "connectedProviders"
        private const val PROVIDER_PRIORITY_KEY = "providerPriority"

        internal fun voiceModelEnabled(context: Context): Boolean =
            context.applicationContext.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
                .getBoolean(VOICE_MODEL_KEY, true)
    }

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val stateLock = Mutex()
    private var companion: BridgethingCompanion? = null
    private var eventsJob: Job? = null
    private var otaJob: Job? = null
    private var deviceMetaJob: Job? = null
    private var webappDocJob: Job? = null

    private val peers = ConcurrentHashMap<String, BridgethingSessionPeer>()

    @Volatile
    private var lastNowPlaying: BridgethingNowPlaying? = null

    private val foregroundGen = java.util.concurrent.atomic.AtomicLong(0)
    private val webappsGen = ConcurrentHashMap<String, Long>()

    private val connectJobs = ConcurrentHashMap<String, Job>()
    private val authStates = ConcurrentHashMap<String, BridgethingAuthState>()
    private val healthStates = ConcurrentHashMap<String, BridgethingServiceHealth>()
    private val connectedIds: MutableSet<String> = java.util.concurrent.ConcurrentHashMap.newKeySet()

    @Volatile
    private var priority: List<String> = emptyList()

    @Volatile
    private var onProvidersChanged: ((Array<BridgethingProviderInfo>) -> Unit)? = null

    @Volatile
    private var onPeerConnected: ((BridgethingSessionPeer) -> Unit)? = null

    @Volatile
    private var onPeerDisconnected: ((String) -> Unit)? = null

    @Volatile
    private var onPeerLinkFailed: ((BridgethingSessionPeer) -> Unit)? = null

    @Volatile
    private var onNowPlayingChanged: ((BridgethingNowPlaying?) -> Unit)? = null

    @Volatile
    private var onAncsAuthStatusChanged: ((String, BridgethingAncsAuthStatus) -> Unit)? = null

    @Volatile
    private var onLog: ((String, String) -> Unit)? = null

    @Volatile
    private var onWebappsChanged: ((BridgethingDeviceWebappsEntry) -> Unit)? = null

    @Volatile
    private var onWebappDocChanged: ((String, String, String, String?) -> Unit)? = null

    @Volatile
    private var onDeviceMetaChanged: ((String, BridgethingDeviceMeta) -> Unit)? = null

    @Volatile
    private var onVoiceModelStateChanged: ((BridgethingVoiceModelState) -> Unit)? = null

    @Volatile
    private var onOtaRunChanged: ((BridgethingOtaRun) -> Unit)? = null
    private var onOtaAvailableChanged: ((BridgethingOtaAvailable) -> Unit)? = null
    private var onOtaPollChanged: ((BridgethingOtaPollStatus) -> Unit)? = null
    private var onResumed: ((BridgethingSessionSnapshot) -> Unit)? = null

    private val prefs by lazy {
        context.applicationContext.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
    }

    @Volatile
    private var logStreamingDesired: Boolean = false
    private var localLogStreamingDesired: Boolean = false

    init {
        scope.launch {
            VoiceModels.states(context).collect { state ->
                safeEmit {
                    if (CompanionHolder.foreground) onVoiceModelStateChanged?.invoke(toRnVoiceModelState(state))
                }
            }
        }
    }

    override suspend fun start() {
        val c = CompanionHolder.ensureStarted(context)
        val firstAttach = stateLock.withLock {
            if (companion != null) return@withLock false
            companion = c
            true
        }
        if (!firstAttach) return

        c.setNowPlayingObserver { np -> safeEmit { handleNowPlaying(np) } }
        c.setAncsAuthStateObserver { deviceId, state -> safeEmit { emitAncsAuthStatus(deviceId, toRnAncsAuthStatus(state)) } }
        reconcileLogObserver(c)
        if (logStreamingDesired) scope.launch { c.setDeviceLogStreaming(true) }
        if (localLogStreamingDesired) c.setLocalLogStreaming(true)
        eventsJob = scope.launch { c.gateway.events.collect { event -> safeEmit { handleGatewayEvent(event) } } }
        otaJob = scope.launch { c.ota.storeChanges.collect { change -> safeEmit { emitOtaStoreChange(change) } } }
        deviceMetaJob = scope.launch {
            c.ota.metaChanged.collect { (id, meta) ->
                safeEmit { if (CompanionHolder.foreground) onDeviceMetaChanged?.invoke(id, toRnDeviceMeta(meta)) }
                c.ota.noteRunMeta(id, meta.appVersion, meta.imageVersion)
                    ?.let { safeEmit { emitOtaStoreChange(OtaStoreChange.Run(it)) } }
            }
        }
        webappDocJob = scope.launch {
            c.gateway.webapp.docChanged.collect { (deviceId, msg) ->
                safeEmit {
                    if (CompanionHolder.foreground) {
                        onWebappDocChanged?.invoke(deviceId, msg.id.toString().lowercase(), msg.key, msg.value)
                    }
                }
            }
        }

        scope.launch { VoiceModels.ensure(context) }
        runCatching { applyCapabilityFlags(loadCapabilityFlags()) }
        runCatching { applyOtaPollConfig(loadOtaPollConfig()) }
        runCatching { applyDeviceAutoResume() }

        CompanionDevicePicker.startObservingPresence(context)
        if (CompanionDevicePicker.associations(context.applicationContext).isNotEmpty()) {
            BridgethingConnectionService.start(context)
        }

        priority = prefs.getString(PROVIDER_PRIORITY_KEY, null)?.split(",")?.filter { it.isNotEmpty() } ?: emptyList()
        stateLock.withLock { companion }?.setProviderPriority(priority)

        val restore = (prefs.getStringSet(CONNECTED_PROVIDERS_KEY, emptySet()) ?: emptySet()).toMutableSet()
        for (reg in registry) if (reg.available && reg.hasCredentials()) restore.add(reg.id)
        for (reg in registry) {
            if (reg.available && restore.contains(reg.id)) runCatching { connectProvider(reg.id) }
        }
    }

    override suspend fun stop() {
        var priorEvents: Job? = null
        var priorOta: Job? = null
        var priorDeviceMeta: Job? = null
        var priorWebappDoc: Job? = null
        var priorCompanion: BridgethingCompanion? = null
        stateLock.withLock {
            priorEvents = eventsJob
            priorOta = otaJob
            priorDeviceMeta = deviceMetaJob
            priorWebappDoc = webappDocJob
            priorCompanion = companion
            companion = null
            eventsJob = null
            otaJob = null
            deviceMetaJob = null
            webappDocJob = null
        }
        priorEvents?.cancel()
        priorOta?.cancel()
        priorDeviceMeta?.cancel()
        priorWebappDoc?.cancel()
        for (job in connectJobs.values) job.cancel()
        connectJobs.clear()
        priorCompanion?.setNowPlayingObserver(null)
        priorCompanion?.setAncsAuthStateObserver(null)
        priorCompanion?.setLogObserver(null)
        peers.clear()
        lastNowPlaying = null
        emitNowPlaying(null)
    }

    override suspend fun availableProviders(): Array<BridgethingProviderInfo> = providerInfos()

    private fun providerInfos(): Array<BridgethingProviderInfo> = registry.map { reg ->
        BridgethingProviderInfo(
            id = reg.id,
            displayName = reg.displayName,
            available = reg.available,
            connected = connectedIds.contains(reg.id),
            authState = authStates[reg.id] ?: idleState(),
            serviceHealth = healthStates[reg.id] ?: toRnServiceHealth(GlueServiceHealth.Ok),
        )
    }.sortedWith(
        compareBy({ priority.indexOf(it.id).let { i -> if (i < 0) Int.MAX_VALUE else i } }, { it.id }),
    ).toTypedArray()

    override suspend fun connectProvider(id: String) {
        connectJobs.remove(id)?.cancel()
        val c = stateLock.withLock { companion } ?: error("session not started")
        val registration = registry.firstOrNull { it.id == id } ?: error("unknown provider $id")
        setAuthState(id, authState(BridgethingAuthKind.PENDING))
        try {
            val glue = registration.factory()
            glue.setAuthObserver { state -> handleGlueAuthState(id, state) }
            glue.setServiceHealthObserver { health -> setServiceHealth(id, toRnServiceHealth(health)) }
            c.attach(glue)
        } catch (e: Throwable) {
            setAuthState(id, authState(BridgethingAuthKind.FAILED, message = e.message ?: e.toString()))
            throw e
        }
    }

    private fun handleGlueAuthState(id: String, state: GlueAuthState) {
        when (state) {
            is GlueAuthState.Pending -> setAuthState(
                id,
                authState(
                    BridgethingAuthKind.PENDING,
                    userCode = state.prompt?.userCode,
                    verificationUrl = state.prompt?.verificationUrl,
                    verificationUrlComplete = state.prompt?.verificationUrlComplete,
                ),
            )
            is GlueAuthState.Authenticated -> {
                connectedIds.add(id)
                persistConnected()
                setAuthState(id, authState(BridgethingAuthKind.AUTHENTICATED))
            }
            is GlueAuthState.Failed -> setAuthState(id, authState(BridgethingAuthKind.FAILED, message = state.reason))
        }
    }

    private fun setAuthState(id: String, state: BridgethingAuthState) {
        authStates[id] = state
        emitProviders()
    }

    private fun setServiceHealth(id: String, health: BridgethingServiceHealth) {
        healthStates[id] = health
        emitProviders()
    }

    private fun persistConnected() {
        prefs.edit().putStringSet(CONNECTED_PROVIDERS_KEY, connectedIds.toSet()).apply()
    }

    override suspend fun cancelAuth(id: String) {
        connectJobs.remove(id)?.cancel()
        connectedIds.remove(id)
        persistConnected()
        stateLock.withLock { companion }?.detach(id)
        setAuthState(id, idleState())
    }

    override suspend fun disconnectProvider(id: String) {
        connectJobs.remove(id)?.cancel()
        connectedIds.remove(id)
        healthStates.remove(id)
        persistConnected()
        runCatching { registry.firstOrNull { it.id == id }?.signOut?.invoke() }
        stateLock.withLock { companion }?.detach(id)
        setAuthState(id, idleState())
    }

    override suspend fun setProviderPriority(ids: Array<String>) {
        priority = ids.toList()
        prefs.edit().putString(PROVIDER_PRIORITY_KEY, priority.joinToString(",")).apply()
        stateLock.withLock { companion }?.setProviderPriority(priority)
        emitProviders()
    }

    override suspend fun snapshot(): BridgethingSessionSnapshot {
        val c = stateLock.withLock { companion }
        val libraryProvider = c?.libraryGlue()?.name
        val ancsStatuses = mutableListOf<BridgethingAncsAuthStatusEntry>()
        val deviceMetaEntries = mutableListOf<BridgethingDeviceMetaEntry>()
        if (c != null) {
            for (id in peers.keys) {
                ancsStatuses.add(
                    BridgethingAncsAuthStatusEntry(
                        deviceId = id,
                        status = toRnAncsAuthStatus(c.currentAncsAuthState()),
                    )
                )
                val meta = c.ota.meta(id) ?: continue
                deviceMetaEntries.add(BridgethingDeviceMetaEntry(deviceId = id, meta = toRnDeviceMeta(meta)))
            }
        }
        val webappEntries = mutableListOf<BridgethingDeviceWebappsEntry>()
        for (peer in peers.values) {
            if (peer.status != BridgethingPeerLinkStatus.CONNECTED) continue
            webappsEntry(peer.id)?.let { webappEntries.add(it) }
        }
        return BridgethingSessionSnapshot(
            hostInfo = rnHostInfo(),
            providers = providerInfos(),
            providerPriority = priority.toTypedArray(),
            libraryProvider = libraryProvider,
            peers = peers.values.toTypedArray(),
            ancsAuthStatuses = ancsStatuses.toTypedArray(),
            nowPlaying = lastNowPlaying,
            deviceMeta = deviceMetaEntries.toTypedArray(),
            capabilityFlags = loadCapabilityFlags(),
            voiceModel = voiceModelState(),
            otaPollConfig = loadOtaPollConfig(),
            webapps = webappEntries.toTypedArray(),
            otaRuns = (c?.ota?.retainedRuns() ?: emptyList()).map { toRnOtaRun(it) }.toTypedArray(),
            otaAvailable = (c?.ota?.retainedAvailable() ?: emptyList()).map { toRnOtaAvailable(it) }.toTypedArray(),
            otaPoll = toRnOtaPollStatus(c?.ota?.retainedPollStatus() ?: OtaPollStatus()),
        )
    }

    override suspend fun deviceLogSnapshot(limit: Double): Array<BridgethingDeviceLogLine> =
        DeviceLogRing.tail(limit.toInt())
            .map { BridgethingDeviceLogLine(seq = it.seq.toDouble(), ts = it.timestampMs, level = it.level, message = it.message) }
            .toTypedArray()

    override suspend fun persistedLogSize(): Double = withContext(Dispatchers.IO) {
        LogStore.retainedBytes().toDouble()
    }

    override suspend fun logArchives(): Array<BridgethingLogArchive> = withContext(Dispatchers.IO) {
        LogStore.archives()
            .map {
                BridgethingLogArchive(
                    id = it.id,
                    startedAt = it.startedAtMs.toDouble(),
                    bytes = it.bytes.toDouble(),
                    pinned = it.pinned,
                    current = it.current,
                )
            }
            .toTypedArray()
    }

    override suspend fun exportLogs(archiveId: String?): String = withContext(Dispatchers.IO) {
        LogExport.writeBundle(context, archiveId).absolutePath
    }

    override suspend fun shareLogs(archiveId: String?): Boolean {
        val file = withContext(Dispatchers.IO) {
            runCatching { LogExport.writeBundle(context, archiveId) }.getOrNull()
        } ?: return false
        return withContext(Dispatchers.Main) { LogExport.share(context, file) }
    }

    override suspend fun deleteLogArchive(archiveId: String): Unit = withContext(Dispatchers.IO) {
        LogStore.delete(archiveId)
    }

    override suspend fun clearPersistedLogs(): Unit = withContext(Dispatchers.IO) { LogStore.clear() }

    override suspend fun companionDebug(): BridgethingCompanionDebug {
        val c = stateLock.withLock { companion }
        val debug = (c?.audibleGlue() ?: c?.libraryGlue())?.debugState() ?: GlueDebugState()
        return BridgethingCompanionDebug(
            authorityPlaybackHeld = debug.authorityPlaybackHeld,
            authorityMetadataHeld = debug.authorityMetadataHeld,
        )
    }

    override suspend fun enableAncsNotifications(deviceId: String): BridgethingAncsSetupResult {
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

    override suspend fun ancsAuthStatus(deviceId: String): BridgethingAncsAuthStatus =
        toRnAncsAuthStatus(stateLock.withLock { companion }?.currentAncsAuthState() ?: AncsAuthState.Unknown)

    override suspend fun listWebapps(deviceId: String): Array<BridgethingWebappInfo> {
        val c = requireCompanion(deviceId)
        val value = unwrapVoid(c.gateway.webapp.list(deviceId), "listWebapps")
        return value.webapps
            .filter { it.role != WebappRole.Launcher }
            .map(::toRnWebappInfo)
            .toTypedArray()
    }

    override suspend fun currentWebapp(deviceId: String): BridgethingActiveWebapp? {
        val c = requireCompanion(deviceId)
        val value = unwrapVoid(c.gateway.webapp.getActive(deviceId), "currentWebapp")
        val id = value.id ?: return null
        return BridgethingActiveWebapp(id = id.toString(), name = value.name)
    }

    override suspend fun installWebapp(deviceId: String, sourceUri: String): BridgethingWebappInfo {
        val c = requireCompanion(deviceId)
        val (archive, isTemporary) = resolveArchive(sourceUri)
        try {
            return when (val result = c.ota.installWebapp(c.gateway, deviceId, archive, provenanceForSideload(sourceUri))) {
                is WebappInstallResult.Installed -> {
                    emitWebappsChanged(deviceId)
                    toRnWebappInfo(result.info)
                }
                is WebappInstallResult.Failed -> throw IllegalStateException("install failed: ${result.reason}")
            }
        } finally {
            if (isTemporary) archive.delete()
        }
    }

    override suspend fun uninstallWebapp(deviceId: String, id: String) {
        val uuid = parseUuid(id)
        val c = requireCompanion(deviceId)
        unwrapWebapp(c.gateway.webapp.uninstall(deviceId, WebappUninstall(uuid)), "uninstallWebapp")
        emitWebappsChanged(deviceId)
    }

    override suspend fun switchWebapp(deviceId: String, id: String) {
        val uuid = parseUuid(id)
        val c = requireCompanion(deviceId)
        unwrapWebapp(c.gateway.webapp.switchTo(deviceId, WebappSwitchTo(uuid)), "switchWebapp")
        emitWebappsChanged(deviceId)
    }

    override suspend fun getWebappSlots(deviceId: String): BridgethingWebappSlots {
        val c = requireCompanion(deviceId)
        return toRnWebappSlots(unwrapWebapp(c.gateway.webapp.getSlots(deviceId), "getWebappSlots"))
    }

    override suspend fun setWebappSlot(deviceId: String, slot: BridgethingWebappSlot, id: String?): BridgethingWebappSlots {
        val c = requireCompanion(deviceId)
        val target = when (slot) {
            BridgethingWebappSlot.LAUNCHER -> WebappSlot.Launcher
            BridgethingWebappSlot.OVERLAY -> WebappSlot.Overlay
        }
        val reply = unwrapWebapp(
            c.gateway.webapp.setSlot(deviceId, WebappSetSlot(target, id?.let { parseUuid(it) })),
            "setWebappSlot",
        )
        emitWebappsChanged(deviceId)
        return toRnWebappSlots(reply)
    }

    override suspend fun webappIcon(deviceId: String, id: String): BridgethingWebappIcon? {
        val uuid = parseUuid(id)
        val c = requireCompanion(deviceId)
        val resolved = c.webappResources.fetch(deviceId, uuid, WebappResourceKind.Icon) ?: return null
        return if (resolved.mime == "image/svg+xml") {
            BridgethingWebappIcon(fileUri = null, svg = resolved.file.readText(), mime = resolved.mime)
        } else {
            BridgethingWebappIcon(fileUri = Uri.fromFile(resolved.file).toString(), svg = null, mime = resolved.mime)
        }
    }

    override suspend fun webappSettingsPage(deviceId: String, id: String): String {
        val uuid = parseUuid(id)
        val c = requireCompanion(deviceId)
        val resolved = c.webappResources.fetch(deviceId, uuid, WebappResourceKind.Settings)
            ?: throw IllegalStateException("webappSettingsPage: resource unavailable")
        return Uri.fromFile(resolved.file).toString()
    }

    override suspend fun listWebappConfig(deviceId: String, id: String): Array<BridgethingConfigEntry> {
        val uuid = parseUuid(id)
        val c = requireCompanion(deviceId)
        val reply = unwrapWebapp(c.gateway.webapp.configList(deviceId, WebappConfigList(uuid)), "listWebappConfig")
        return reply.entries.map { BridgethingConfigEntry(it.key, it.value) }.toTypedArray()
    }

    override suspend fun setWebappConfigField(deviceId: String, id: String, key: String, value: String) {
        val uuid = parseUuid(id)
        val c = requireCompanion(deviceId)
        unwrapWebapp(c.gateway.webapp.configSet(deviceId, WebappConfigSet(uuid, key, value)), "setWebappConfigField")
    }

    override suspend fun deleteWebappConfigField(deviceId: String, id: String, key: String) {
        val uuid = parseUuid(id)
        val c = requireCompanion(deviceId)
        unwrapWebapp(c.gateway.webapp.configDelete(deviceId, WebappConfigDelete(uuid, key)), "deleteWebappConfigField")
    }

    override suspend fun getWebappDoc(deviceId: String, id: String, key: String): String? {
        val uuid = parseUuid(id)
        val c = requireCompanion(deviceId)
        return unwrapWebapp(c.gateway.webapp.docGet(deviceId, WebappDocGet(uuid, key)), "getWebappDoc").value
    }

    override suspend fun listWebappDoc(deviceId: String, id: String): Array<BridgethingDocEntry> {
        val uuid = parseUuid(id)
        val c = requireCompanion(deviceId)
        val reply = unwrapWebapp(c.gateway.webapp.docList(deviceId, WebappDocList(uuid)), "listWebappDoc")
        return reply.entries.map { BridgethingDocEntry(it.key, it.value) }.toTypedArray()
    }

    override suspend fun setWebappDoc(deviceId: String, id: String, key: String, value: String) {
        val uuid = parseUuid(id)
        val c = requireCompanion(deviceId)
        unwrapWebapp(c.gateway.webapp.docSet(deviceId, WebappDocSet(uuid, key, value)), "setWebappDoc")
    }

    override suspend fun deleteWebappDoc(deviceId: String, id: String, key: String) {
        val uuid = parseUuid(id)
        val c = requireCompanion(deviceId)
        unwrapWebapp(c.gateway.webapp.docDelete(deviceId, WebappDocDelete(uuid, key)), "deleteWebappDoc")
    }

    override suspend fun setCapabilityFlags(flags: BridgethingCapabilityFlags) {
        saveCapabilityFlags(flags)
        applyCapabilityFlags(flags)
    }

    private suspend fun applyCapabilityFlags(flags: BridgethingCapabilityFlags) {
        VoiceModels.setEnabled(context, flags.voiceModel)
        stateLock.withLock { companion }?.setCapabilityFlags(
            CompanionCapabilityFlags(
                geo = flags.geo,
                notifications = flags.notifications,
                netFetch = flags.netFetch,
                netWs = flags.netWs,
                audioTts = flags.audioTts,
                voiceModel = flags.voiceModel,
            )
        )
    }

    override suspend fun voiceModelState(): BridgethingVoiceModelState =
        toRnVoiceModelState(VoiceModels.state(context))

    private fun toRnVoiceModelState(state: ModelBundleState): BridgethingVoiceModelState = when (state) {
        is ModelBundleState.Absent -> BridgethingVoiceModelState(
            status = BridgethingVoiceModelStatus.ABSENT,
            receivedBytes = 0.0,
            totalBytes = 0.0,
            version = null,
            error = null,
        )
        is ModelBundleState.Downloading -> BridgethingVoiceModelState(
            status = BridgethingVoiceModelStatus.DOWNLOADING,
            receivedBytes = state.received.toDouble(),
            totalBytes = state.total.toDouble(),
            version = null,
            error = null,
        )
        is ModelBundleState.Ready -> BridgethingVoiceModelState(
            status = BridgethingVoiceModelStatus.READY,
            receivedBytes = 0.0,
            totalBytes = 0.0,
            version = state.version,
            error = null,
        )
        is ModelBundleState.Failed -> BridgethingVoiceModelState(
            status = BridgethingVoiceModelStatus.FAILED,
            receivedBytes = 0.0,
            totalBytes = 0.0,
            version = null,
            error = state.reason,
        )
    }

    override suspend fun setDeviceAutoResume(deviceId: String, enabled: Boolean) {
        prefs.edit().putBoolean("$AUTO_RESUME_PREFIX$deviceId", enabled).apply()
        stateLock.withLock { companion }?.setDeviceAutoResume(deviceId, enabled)
    }

    override suspend fun isDeviceAutoResumeEnabled(deviceId: String): Boolean =
        prefs.getBoolean("$AUTO_RESUME_PREFIX$deviceId", true)

    private suspend fun applyDeviceAutoResume() {
        val companion = stateLock.withLock { companion } ?: return
        for ((key, value) in prefs.all) {
            if (key.startsWith(AUTO_RESUME_PREFIX) && value is Boolean) {
                companion.setDeviceAutoResume(key.removePrefix(AUTO_RESUME_PREFIX), value)
            }
        }
    }

    override suspend fun setOtaPollConfig(config: BridgethingOtaPollConfig?) {
        saveOtaPollConfig(config)
        applyOtaPollConfig(config)
    }

    private suspend fun applyOtaPollConfig(config: BridgethingOtaPollConfig?) {
        val ota = stateLock.withLock { companion }?.ota ?: return
        if (config == null) {
            ota.setPollConfig(null)
        } else {
            ota.setPollConfig(
                KOtaPollConfig(
                    rootUrl = config.rootUrl ?: "https://ota.bridgething.com",
                    intervalSeconds = config.intervalSeconds.toLong().coerceAtLeast(60L),
                    cacheDirectory = context.cacheDir,
                    autoPush = config.autoPush,
                )
            )
        }
    }

    override suspend fun checkForOtaUpdate(rootUrl: String?) {
        stateLock.withLock { companion }?.ota?.checkNow(otaRootUrl(rootUrl))
    }

    override suspend fun fetchOtaManifest(rootUrl: String?): BridgethingOtaManifest {
        val ota = stateLock.withLock { companion }?.ota
            ?: return BridgethingOtaManifest(updatedAt = "", channels = emptyArray())
        return toRnOtaManifest(ota.discoverManifest(otaRootUrl(rootUrl)))
    }

    override suspend fun applyOtaUpdate(deviceId: String, channel: String, version: String, rootUrl: String?) {
        stateLock.withLock { companion }?.ota?.applyVersion(deviceId, channel, version, otaRootUrl(rootUrl))
    }

    override suspend fun installWebappFromUrl(
        deviceId: String,
        url: String,
        sha256: String,
        size: Double,
        provenance: String?,
        webappId: String?,
        webappName: String?,
    ): BridgethingWebappInfo {
        val c = requireCompanion(deviceId)
        val result = c.ota.installWebappFromUrl(
            gateway = c.gateway,
            deviceId = deviceId,
            url = url,
            sha256 = sha256.lowercase(),
            size = size.toLong(),
            provenance = provenance,
            cacheDir = context.cacheDir ?: java.io.File(System.getProperty("java.io.tmpdir") ?: "."),
            webappId = webappId,
            webappName = webappName,
        )
        return when (result) {
            is WebappInstallResult.Installed -> {
                emitWebappsChanged(deviceId)
                toRnWebappInfo(result.info)
            }
            is WebappInstallResult.Failed -> throw IllegalStateException("install failed: ${result.reason}")
        }
    }

    override suspend fun reconnectPeer(deviceId: String) {
        stateLock.withLock { companion }?.gateway?.reconnect(deviceId)
    }

    override suspend fun deviceSetNickname(deviceId: String, nickname: String) {
        val c = requireCompanion(deviceId)
        when (val result = c.gateway.system.deviceSetNickname(deviceId, DeviceSetNickname(nickname))) {
            is RequestResult.Ok -> Unit
            is RequestResult.DomainErr -> throw IllegalStateException("nickname rejected: ${result.error.reason}")
            is RequestResult.ProtocolErr -> throw IllegalStateException("deviceSetNickname: ${result.error}")
        }
    }

    private fun otaRootUrl(raw: String?): String = raw ?: "https://ota.bridgething.com"

    private fun toRnOtaManifest(m: OtaDiscoverManifest): BridgethingOtaManifest {
        val channels = m.channels.map { (slug, ch) ->
            val releases = ch.releases.mapNotNull { v ->
                val composite = OtaCompositeVersion.parse(v) ?: return@mapNotNull null
                val rel = m.releases[v]
                BridgethingOtaRelease(
                    version = v,
                    daemonVersion = composite.daemon,
                    imageVersion = composite.image,
                    yanked = rel?.yanked != null,
                    deprecated = rel?.deprecated ?: false,
                )
            }.toTypedArray()
            BridgethingOtaChannelInfo(
                slug = slug,
                name = ch.name,
                stability = ch.stability,
                isDefault = ch.isDefault,
                latest = ch.latest,
                releases = releases,
            )
        }.toTypedArray()
        return BridgethingOtaManifest(updatedAt = m.updatedAt, channels = channels)
    }

    private fun toRnDeviceMeta(meta: BridgeThingMeta): BridgethingDeviceMeta = BridgethingDeviceMeta(
        daemonVersion = meta.appVersion,
        libbridgethingVersion = meta.libbridgethingVersion,
        imageVersion = meta.imageVersion,
        appName = meta.appName,
        osName = meta.osName,
        osVersion = meta.osVersion,
        channel = meta.channel,
        modelName = meta.modelName,
        serialNumber = meta.serialNumber,
        nickname = meta.nickname,
    )

    private fun rnHostInfo(): BridgethingHostInfo {
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

    private fun makeHostInfo(): HostInfo = CompanionHolder.makeHostInfo(context)

    // --- native-authoritative persistence ---

    private fun loadCapabilityFlags(): BridgethingCapabilityFlags {
        if (!prefs.getBoolean("caps.configured", false)) {
            return BridgethingCapabilityFlags(
                geo = true, notifications = true, netFetch = true, netWs = true, audioTts = true,
                voiceModel = true,
            )
        }
        return BridgethingCapabilityFlags(
            geo = prefs.getBoolean("caps.geo", true),
            notifications = prefs.getBoolean("caps.notifications", true),
            netFetch = prefs.getBoolean("caps.netFetch", true),
            netWs = prefs.getBoolean("caps.netWs", true),
            audioTts = prefs.getBoolean("caps.audioTts", true),
            voiceModel = prefs.getBoolean(VOICE_MODEL_KEY, true),
        )
    }

    private fun saveCapabilityFlags(f: BridgethingCapabilityFlags) {
        prefs.edit()
            .putBoolean("caps.configured", true)
            .putBoolean("caps.geo", f.geo)
            .putBoolean("caps.notifications", f.notifications)
            .putBoolean("caps.netFetch", f.netFetch)
            .putBoolean("caps.netWs", f.netWs)
            .putBoolean("caps.audioTts", f.audioTts)
            .putBoolean(VOICE_MODEL_KEY, f.voiceModel)
            .apply()
    }

    private fun loadOtaPollConfig(): BridgethingOtaPollConfig? {
        if (!prefs.getBoolean("ota.configured", false)) {
            return BridgethingOtaPollConfig(
                intervalSeconds = 3600.0,
                autoPush = true,
                rootUrl = null,
            )
        }
        val root = prefs.getString("ota.rootUrl", null)
        return BridgethingOtaPollConfig(
            intervalSeconds = prefs.getLong("ota.intervalSeconds", 3600L).toDouble(),
            autoPush = prefs.getBoolean("ota.autoPush", true),
            rootUrl = if (root.isNullOrEmpty()) null else root,
        )
    }

    private fun saveOtaPollConfig(config: BridgethingOtaPollConfig?) {
        if (config == null) {
            prefs.edit().putBoolean("ota.configured", false).apply()
            return
        }
        prefs.edit()
            .putBoolean("ota.configured", true)
            .putLong("ota.intervalSeconds", config.intervalSeconds.toLong())
            .putBoolean("ota.autoPush", config.autoPush)
            .putString("ota.rootUrl", config.rootUrl)
            .apply()
    }

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

    override suspend fun forgetCompanionDevice(mac: String) {
        val ctx = context.applicationContext
        CompanionDevicePicker.forget(ctx, mac)
        runCatching { CompanionHolder.adapter?.disconnect(mac) }
        if (CompanionDevicePicker.associations(ctx).isEmpty()) {
            BridgethingConnectionService.stop(ctx)
        }
    }

    override suspend fun isIgnoringBatteryOptimizations(): Boolean {
        val ctx = context.applicationContext
        val pm = ctx.getSystemService(Context.POWER_SERVICE) as? android.os.PowerManager ?: return false
        return pm.isIgnoringBatteryOptimizations(ctx.packageName)
    }

    override suspend fun requestIgnoreBatteryOptimizations() {
        val ctx = context.applicationContext
        @Suppress("BatteryLife")
        val intent = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS)
            .setData(Uri.parse("package:${ctx.packageName}"))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
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
        android.os.Process.killProcess(android.os.Process.myPid())
    }

    override suspend fun presentPairPicker(): BridgethingBtDevice? {
        val picked = CompanionDevicePicker.pick(context.applicationContext) ?: return null
        CompanionDevicePicker.startObservingPresence(context)
        BridgethingConnectionService.start(context)

        val bonded = CompanionDevicePicker.awaitBond(context.applicationContext, picked.address)
        return BridgethingBtDevice(
            address = picked.address,
            name = picked.name,
            bondState = if (bonded) BridgethingBtBondState.BONDED else BridgethingBtBondState.NONE,
            isCarThing = picked.isCarThing,
        )
    }

    override fun setOnProvidersChanged(callback: (Array<BridgethingProviderInfo>) -> Unit) { onProvidersChanged = callback }
    override fun setOnPeerConnected(callback: (BridgethingSessionPeer) -> Unit) { onPeerConnected = callback }
    override fun setOnPeerDisconnected(callback: (String) -> Unit) { onPeerDisconnected = callback }
    override fun setOnPeerLinkFailed(callback: (BridgethingSessionPeer) -> Unit) { onPeerLinkFailed = callback }
    override fun setOnNowPlayingChanged(callback: (BridgethingNowPlaying?) -> Unit) { onNowPlayingChanged = callback }
    override fun setOnAncsAuthStatusChanged(callback: (String, BridgethingAncsAuthStatus) -> Unit) { onAncsAuthStatusChanged = callback }
    override fun setOnLog(callback: (String, String) -> Unit) { onLog = callback }

    override fun setLogStreamingEnabled(enabled: Boolean) {
        logStreamingDesired = enabled
        val c = companion ?: return
        reconcileLogObserver(c)
        scope.launch { c.setDeviceLogStreaming(enabled) }
    }

    override fun setLocalLogStreamingEnabled(enabled: Boolean) {
        localLogStreamingDesired = enabled
        val c = companion ?: return
        reconcileLogObserver(c)
        c.setLocalLogStreaming(enabled)
    }

    private fun reconcileLogObserver(c: BridgethingCompanion) {
        if (logStreamingDesired || localLogStreamingDesired) {
            c.setLogObserver { level, msg -> safeEmit { if (CompanionHolder.foreground) onLog?.invoke(level.raw, msg) } }
        } else {
            c.setLogObserver(null)
        }
    }

    override fun setOnWebappsChanged(callback: (BridgethingDeviceWebappsEntry) -> Unit) { onWebappsChanged = callback }
    override fun setOnWebappDocChanged(callback: (String, String, String, String?) -> Unit) { onWebappDocChanged = callback }
    override fun setOnDeviceMetaChanged(callback: (String, BridgethingDeviceMeta) -> Unit) { onDeviceMetaChanged = callback }
    override fun setOnVoiceModelStateChanged(callback: (BridgethingVoiceModelState) -> Unit) { onVoiceModelStateChanged = callback }
    override fun setOnOtaRunChanged(callback: (BridgethingOtaRun) -> Unit) { onOtaRunChanged = callback }

    override fun setOnOtaAvailableChanged(callback: (BridgethingOtaAvailable) -> Unit) { onOtaAvailableChanged = callback }

    override fun setOnOtaPollChanged(callback: (BridgethingOtaPollStatus) -> Unit) { onOtaPollChanged = callback }

    override fun setOnResumed(callback: (BridgethingSessionSnapshot) -> Unit) { onResumed = callback }

    private suspend fun requireCompanion(deviceId: String): BridgethingCompanion {
        val c = stateLock.withLock { companion } ?: throw IllegalStateException("session not started")
        if (!peers.containsKey(deviceId)) throw IllegalStateException("no peer connected: $deviceId")
        return c
    }

    private fun parseUuid(id: String): UUID = try {
        UUID.fromString(id)
    } catch (e: IllegalArgumentException) {
        throw IllegalArgumentException("invalid uuid: $id")
    }

    private fun emitWebappsChanged(deviceId: String) {
        val gen = webappsGen.compute(deviceId) { _, prev -> (prev ?: 0L) + 1L }
        scope.launch {
            repeat(WEBAPPS_READ_ATTEMPTS) { attempt ->
                if (webappsGen[deviceId] != gen) return@launch
                val entry = webappsEntry(deviceId)
                if (entry != null) {
                    if (webappsGen[deviceId] != gen) return@launch
                    safeEmit { if (CompanionHolder.foreground) onWebappsChanged?.invoke(entry) }
                    return@launch
                }
                if (attempt < WEBAPPS_READ_ATTEMPTS - 1) delay(400L * (attempt + 1))
            }
        }
    }

    private suspend fun webappsEntry(deviceId: String): BridgethingDeviceWebappsEntry? {
        val list = runCatching { listWebapps(deviceId) }.getOrNull() ?: return null
        val active = runCatching { currentWebapp(deviceId) }.getOrNull()
        return BridgethingDeviceWebappsEntry(deviceId = deviceId, webapps = list, active = active)
    }

    private fun emitOtaStoreChange(change: OtaStoreChange) {
        if (!CompanionHolder.foreground) return
        when (change) {
            is OtaStoreChange.Run -> onOtaRunChanged?.invoke(toRnOtaRun(change.run))
            is OtaStoreChange.Available -> onOtaAvailableChanged?.invoke(toRnOtaAvailable(change.available))
            is OtaStoreChange.Poll -> onOtaPollChanged?.invoke(toRnOtaPollStatus(change.status))
        }
    }

    public fun resumeForeground() {
        val gen = foregroundGen.incrementAndGet()
        scope.launch { VoiceModels.ensure(context) }
        scope.launch {
            val snap = snapshot()
            if (foregroundGen.get() != gen) return@launch
            CompanionHolder.foreground = true
            safeEmit { onResumed?.invoke(snap) }
        }
    }

    init {
        CompanionHolder.onForeground = { resumeForeground() }
        CompanionHolder.onBackground = { foregroundGen.incrementAndGet() }
    }

    override suspend fun dismissOtaRun(deviceId: String) {
        stateLock.withLock { companion }?.ota?.dismissRun(deviceId)
    }

    private fun <T> unwrapVoid(result: RequestResult<T, Nothing>, label: String): T = when (result) {
        is RequestResult.Ok -> result.response
        is RequestResult.DomainErr -> throw IllegalStateException("$label: unreachable")
        is RequestResult.ProtocolErr -> throw IllegalStateException("$label: ${result.error}")
    }

    private fun <T> unwrapWebapp(result: RequestResult<T, WebappError>, label: String): T = when (result) {
        is RequestResult.Ok -> result.response
        is RequestResult.DomainErr -> throw IllegalStateException("$label: ${result.error}")
        is RequestResult.ProtocolErr -> throw IllegalStateException("$label: ${result.error}")
    }

    private fun provenanceForSideload(sourceUri: String): String? =
        when (runCatching { URI(sourceUri).scheme?.lowercase() }.getOrNull()) {
            "http", "https" -> sourceUri
            else -> null
        }

    private suspend fun resolveArchive(sourceUri: String): Pair<File, Boolean> = withContext(Dispatchers.IO) {
        when (URI(sourceUri).scheme?.lowercase()) {
            "file" -> File(URI(sourceUri)) to false
            "http", "https" -> {
                val conn = (URL(sourceUri).openConnection() as HttpURLConnection).apply {
                    connectTimeout = 30_000
                    readTimeout = 30_000
                    instanceFollowRedirects = true
                }
                try {
                    val code = conn.responseCode
                    if (code !in 200..299) throw IllegalStateException("download failed: $code")
                    val temp = File.createTempFile("webapp-install", ".zip", context.cacheDir)
                    conn.inputStream.use { input -> temp.outputStream().use { out -> input.copyTo(out) } }
                    temp to true
                } finally {
                    conn.disconnect()
                }
            }
            else -> throw IllegalArgumentException("invalid archive uri")
        }
    }


    private fun toRnWebappInfo(info: WebappInfo): BridgethingWebappInfo = BridgethingWebappInfo(
        id = info.id.toString(),
        name = info.name,
        source = if (info.source == WebappSource.Builtin) BridgethingWebappSource.BUILTIN else BridgethingWebappSource.INSTALLED,
        role = if (info.role == WebappRole.Launcher) BridgethingWebappRole.LAUNCHER else BridgethingWebappRole.STANDARD,
        version = info.version,
        provenance = info.provenance,
        description = info.description,
        iconHash = info.iconHash,
        settingsHash = info.settingsHash,
        overlayHash = info.overlayHash,
        config = info.config.map(::toRnConfigField).toTypedArray(),
        permissions = info.permissions.toTypedArray(),
    )

    private fun toRnWebappSlots(slots: WebappSlots): BridgethingWebappSlots = BridgethingWebappSlots(
        launcher = slots.launcher?.toString(),
        overlay = slots.overlay?.toString(),
    )

    private fun toRnConfigField(field: ConfigField): BridgethingConfigField = when (field) {
        is ConfigField.String -> field.data.let { f ->
            BridgethingConfigField(
                BridgethingConfigKind.STRING, f.key, f.label, f.pattern,
                f.minLength?.toDouble(), f.maxLength?.toDouble(), null, null, null, null, f.default,
            )
        }
        is ConfigField.Secret -> field.data.let { f ->
            BridgethingConfigField(
                BridgethingConfigKind.SECRET, f.key, f.label, f.pattern,
                f.minLength?.toDouble(), f.maxLength?.toDouble(), null, null, null, null, f.default,
            )
        }
        is ConfigField.Number -> field.data.let { f ->
            BridgethingConfigField(
                BridgethingConfigKind.NUMBER, f.key, f.label, null, null, null,
                f.min, f.max, f.step, null, f.default?.toString(),
            )
        }
        is ConfigField.Boolean -> field.data.let { f ->
            BridgethingConfigField(
                BridgethingConfigKind.BOOLEAN, f.key, f.label, null, null, null,
                null, null, null, null, f.default?.let { if (it) "true" else "false" },
            )
        }
        is ConfigField.Enum -> field.data.let { f ->
            BridgethingConfigField(
                BridgethingConfigKind.ENUM, f.key, f.label, null, null, null,
                null, null, null, f.choices.toTypedArray(), f.default,
            )
        }
    }

    private fun handleGatewayEvent(event: GatewayEvent) {
        when (event) {
            is GatewayEvent.Connected -> {
                val peer = BridgethingSessionPeer(
                    id = event.device.id,
                    name = event.device.name,
                    status = BridgethingPeerLinkStatus.CONNECTED,
                    linkError = null,
                )
                peers[event.device.id] = peer
                if (CompanionHolder.foreground) onPeerConnected?.invoke(peer)
            }
            is GatewayEvent.Disconnected -> {
                peers.remove(event.deviceId)
                webappsGen.remove(event.deviceId)
                if (CompanionHolder.foreground) onPeerDisconnected?.invoke(event.deviceId)
            }
            is GatewayEvent.LinkFailed -> {
                val peer = BridgethingSessionPeer(
                    id = event.device.id,
                    name = event.device.name,
                    status = BridgethingPeerLinkStatus.LINKFAILED,
                    linkError = event.reason,
                )
                peers[event.device.id] = peer
                if (CompanionHolder.foreground) onPeerLinkFailed?.invoke(peer)
            }
            is GatewayEvent.Message -> {
                val data = event.message.data
                if (data is BridgeToGatewayMsgData.Version) {
                    if (CompanionHolder.foreground) onDeviceMetaChanged?.invoke(event.deviceId, toRnDeviceMeta(data.data))
                }
            }
            is GatewayEvent.DecodeError -> {
                if (CompanionHolder.foreground) onLog?.invoke("warn", "[${event.deviceId}] decode error: ${event.description}")
            }
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
        if (mapped == lastNowPlaying) return
        lastNowPlaying = mapped
        emitNowPlaying(mapped)
    }

    private inline fun safeEmit(block: () -> Unit) {
        try {
            block()
        } catch (e: CancellationException) {
            throw e
        } catch (t: Throwable) {
            // dropped: stale callback from a torn-down runtime
        }
    }

    private fun emitProviders() { if (CompanionHolder.foreground) onProvidersChanged?.invoke(providerInfos()) }
    private fun toRnServiceHealth(health: GlueServiceHealth): BridgethingServiceHealth = when (health) {
        is GlueServiceHealth.Ok -> BridgethingServiceHealth(BridgethingServiceHealthKind.OK, null)
        is GlueServiceHealth.RateLimited ->
            BridgethingServiceHealth(BridgethingServiceHealthKind.RATELIMITED, health.retryAfterSeconds.toDouble())
        is GlueServiceHealth.Unreachable -> BridgethingServiceHealth(BridgethingServiceHealthKind.UNREACHABLE, null)
    }
    private fun emitNowPlaying(np: BridgethingNowPlaying?) { if (CompanionHolder.foreground) onNowPlayingChanged?.invoke(np) }
    private fun emitAncsAuthStatus(deviceId: String, status: BridgethingAncsAuthStatus) { if (CompanionHolder.foreground) onAncsAuthStatusChanged?.invoke(deviceId, status) }

    private fun idleState() = authState(BridgethingAuthKind.IDLE)

    private fun authState(
        kind: BridgethingAuthKind,
        message: String? = null,
        userCode: String? = null,
        verificationUrl: String? = null,
        verificationUrlComplete: String? = null,
    ): BridgethingAuthState = BridgethingAuthState(
        kind = kind,
        userCode = userCode,
        verificationUrl = verificationUrl,
        verificationUrlComplete = verificationUrlComplete,
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

    private fun toRnOtaRunPhase(p: OtaRunPhase): BridgethingOtaPhase = when (p) {
        OtaRunPhase.IDLE -> BridgethingOtaPhase.IDLE
        OtaRunPhase.DOWNLOADING -> BridgethingOtaPhase.DOWNLOADING
        OtaRunPhase.STREAMING -> BridgethingOtaPhase.STREAMING
        OtaRunPhase.VERIFYING -> BridgethingOtaPhase.VERIFYING
        OtaRunPhase.WRITING -> BridgethingOtaPhase.WRITING
        OtaRunPhase.CONFIRMING -> BridgethingOtaPhase.CONFIRMING
        OtaRunPhase.REBOOT -> BridgethingOtaPhase.REBOOT
        OtaRunPhase.COMPLETED -> BridgethingOtaPhase.COMPLETED
        OtaRunPhase.FAILED -> BridgethingOtaPhase.FAILED
    }

    private fun toRnOtaOutcome(o: OtaRunOutcome): BridgethingOtaOutcome = when (o) {
        OtaRunOutcome.SUCCEEDED -> BridgethingOtaOutcome.SUCCEEDED
        OtaRunOutcome.FAILED -> BridgethingOtaOutcome.FAILED
        OtaRunOutcome.CANCELLED -> BridgethingOtaOutcome.CANCELLED
    }

    private fun toRnOtaRun(run: OtaRun): BridgethingOtaRun = BridgethingOtaRun(
        runId = run.runId,
        deviceId = run.deviceId,
        otaKind = toRnOtaKind(run.kind),
        phase = toRnOtaRunPhase(run.phase),
        steps = run.steps.map {
            BridgethingOtaStep(it.id.toDouble(), toRnStepKind(it.kind), it.label, it.bytes.toDouble())
        }.toTypedArray(),
        stepId = run.stepId.toDouble(),
        startedAt = run.startedAt.toDouble(),
        phaseStartedAt = run.phaseStartedAt.toDouble(),
        stageReceived = run.stageReceived?.toDouble(),
        stageTotal = run.stageTotal?.toDouble(),
        ratePerSec = run.ratePerSec,
        dwlPercent = run.dwlPercent?.toDouble(),
        outcome = run.outcome?.let { toRnOtaOutcome(it) },
        error = run.error,
        releaseVersion = run.releaseVersion,
        daemonVersion = run.daemonVersion,
        imageVersion = run.imageVersion,
        webappId = run.webappId,
        webappName = run.webappName,
    )

    private fun toRnOtaAvailable(a: OtaAvailable): BridgethingOtaAvailable = BridgethingOtaAvailable(
        deviceId = a.deviceId,
        releaseVersion = a.releaseVersion,
        daemonVersion = a.daemonVersion,
        imageVersion = a.imageVersion,
    )

    private fun toRnOtaPollStatus(s: OtaPollStatus): BridgethingOtaPollStatus =
        BridgethingOtaPollStatus(lastPolledAt = s.lastPolledAt, error = s.error)

    private fun toRnOtaKind(k: OtaKind): BridgethingOtaKind = when (k) {
        OtaKind.Image -> BridgethingOtaKind.IMAGE
        OtaKind.Daemon -> BridgethingOtaKind.DAEMON
        OtaKind.BuiltinWebapp -> BridgethingOtaKind.BUILTINWEBAPP
        OtaKind.InstalledWebapp -> BridgethingOtaKind.INSTALLEDWEBAPP
        OtaKind.WakewordModel -> BridgethingOtaKind.WAKEWORDMODEL
    }

    private fun toRnStepKind(k: OtaStepKind): BridgethingOtaStepKind = when (k) {
        OtaStepKind.DOWNLOAD -> BridgethingOtaStepKind.DOWNLOAD
        OtaStepKind.STREAM -> BridgethingOtaStepKind.STREAM
        OtaStepKind.APPLY -> BridgethingOtaStepKind.APPLY
        OtaStepKind.REBOOT -> BridgethingOtaStepKind.REBOOT
    }
}
