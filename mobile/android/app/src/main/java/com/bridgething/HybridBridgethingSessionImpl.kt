package com.bridgething

import android.content.Context
import android.content.Intent
import android.net.Uri
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
import com.margelo.nitro.bridgething.session.BridgethingCatalogEvent
import com.margelo.nitro.bridgething.session.BridgethingCatalogEventKind
import com.margelo.nitro.bridgething.session.BridgethingCatalogPollConfig
import com.margelo.nitro.bridgething.session.BridgethingOtaChannelInfo
import com.margelo.nitro.bridgething.session.BridgethingOtaEvent
import com.margelo.nitro.bridgething.session.BridgethingOtaEventKind
import com.margelo.nitro.bridgething.session.BridgethingOtaKind
import com.margelo.nitro.bridgething.session.BridgethingOtaManifest
import com.margelo.nitro.bridgething.session.BridgethingOtaPhase
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
import com.margelo.nitro.bridgething.session.BridgethingWebappIcon
import com.margelo.nitro.bridgething.session.BridgethingWebappInfo
import com.margelo.nitro.bridgething.session.BridgethingWebappRole
import com.margelo.nitro.bridgething.session.BridgethingWebappSource
import com.bridgething.companion.AncsSetupKind
import com.bridgething.companion.BridgethingCompanion
import com.bridgething.companion.BridgethingCompanionVersion
import com.bridgething.companion.CatalogAppListing
import com.bridgething.companion.CatalogAppUpdate
import com.bridgething.companion.CatalogEvent
import com.bridgething.companion.CatalogPollConfig as KCatalogPollConfig
import com.bridgething.companion.CompanionCapabilityFlags
import com.bridgething.companion.CompanionLogLevel
import com.bridgething.companion.DeviceLogRing
import com.bridgething.companion.HostInfo
import com.bridgething.companion.OtaCompositeVersion
import com.bridgething.companion.OtaDiscoverManifest
import com.bridgething.companion.OtaPhaseSnapshot
import com.bridgething.companion.OtaPollConfig as KOtaPollConfig
import com.bridgething.companion.OtaPollEvent
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
import com.bridgething.schema.WebappSwitchTo
import com.bridgething.schema.WebappUninstall
import java.io.File
import java.net.HttpURLConnection
import java.net.URI
import java.net.URL
import java.nio.ByteBuffer
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.Json

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

        private const val REQUEST_DIALER_ROLE = 0xBA02
        private const val AUTO_RESUME_PREFIX = "autoresume."
    }

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val stateLock = Mutex()
    private var companion: BridgethingCompanion? = null
    private var eventsJob: Job? = null
    private var otaJob: Job? = null
    private var deviceMetaJob: Job? = null
    private var catalogJob: Job? = null
    private var webappDocJob: Job? = null
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
    private var onServiceHealthChanged: ((BridgethingServiceHealth) -> Unit)? = null

    @Volatile
    private var onPeerConnected: ((BridgethingSessionPeer) -> Unit)? = null

    @Volatile
    private var onPeerDisconnected: ((String) -> Unit)? = null

    @Volatile
    private var onPeerLinkFailed: ((BridgethingSessionPeer) -> Unit)? = null

    @Volatile
    private var onNowPlayingChanged: ((BridgethingNowPlaying?) -> Unit)? = null

    @Volatile
    private var onAncsAuthStatusChanged: ((BridgethingAncsAuthStatus) -> Unit)? = null

    @Volatile
    private var onLog: ((String, String) -> Unit)? = null

    @Volatile
    private var onWebappsChanged: ((String) -> Unit)? = null

    @Volatile
    private var onWebappDocChanged: ((String, String, String, String?) -> Unit)? = null

    @Volatile
    private var onDeviceMetaChanged: ((String, BridgethingDeviceMeta) -> Unit)? = null

    @Volatile
    private var onOtaEvent: ((BridgethingOtaEvent) -> Unit)? = null

    @Volatile
    private var onCatalogEvent: ((BridgethingCatalogEvent) -> Unit)? = null

    @Volatile
    private var lastAuthState: BridgethingAuthState = idleState()

    @Volatile
    private var lastServiceHealth: BridgethingServiceHealth = toRnServiceHealth(GlueServiceHealth.Ok)

    private val prefs by lazy {
        context.applicationContext.getSharedPreferences("bridgething.session", Context.MODE_PRIVATE)
    }

    @Volatile
    private var logStreamingDesired: Boolean = false
    private var localLogStreamingDesired: Boolean = false

    override suspend fun start() {
        val c = CompanionHolder.ensureStarted(context)
        val firstAttach = stateLock.withLock {
            if (companion != null) return@withLock false
            companion = c
            true
        }
        if (!firstAttach) return

        c.setNowPlayingObserver { np -> safeEmit { handleNowPlaying(np) } }
        c.setAncsAuthStateObserver { state -> safeEmit { emitAncsAuthStatus(toRnAncsAuthStatus(state)) } }
        reconcileLogObserver(c)
        if (logStreamingDesired) scope.launch { c.setDeviceLogStreaming(true) }
        if (localLogStreamingDesired) c.setLocalLogStreaming(true)
        eventsJob = scope.launch { c.gateway.events.collect { event -> safeEmit { handleGatewayEvent(event) } } }
        otaJob = scope.launch { c.ota.events.collect { ev -> safeEmit { if (CompanionHolder.foreground) onOtaEvent?.invoke(toRnOtaEvent(ev)) } } }
        deviceMetaJob = scope.launch {
            c.ota.metaChanged.collect { (id, meta) ->
                safeEmit { if (CompanionHolder.foreground) onDeviceMetaChanged?.invoke(id, toRnDeviceMeta(meta)) }
            }
        }
        catalogJob = scope.launch { c.catalog.events.collect { ev -> safeEmit { if (CompanionHolder.foreground) onCatalogEvent?.invoke(toRnCatalogEvent(ev)) } } }
        webappDocJob = scope.launch {
            c.gateway.webapp.docChanged.collect { (deviceId, msg) ->
                safeEmit {
                    if (CompanionHolder.foreground) {
                        onWebappDocChanged?.invoke(deviceId, msg.id.toString().lowercase(), msg.key, msg.value)
                    }
                }
            }
        }

        runCatching { applyCapabilityFlags(loadCapabilityFlags()) }
        runCatching { applyOtaPollConfig(loadOtaPollConfig()) }
        runCatching { applyDeviceAutoResume() }

        CompanionDevicePicker.startObservingPresence(context)
        if (CompanionDevicePicker.associations(context.applicationContext).isNotEmpty()) {
            BridgethingConnectionService.start(context)
        }

        registry.firstOrNull { it.available && it.hasCredentials() }?.let {
            runCatching { setActiveProvider(it.id) }
        }
    }

    override suspend fun stop() {
        var priorEvents: Job? = null
        var priorOta: Job? = null
        var priorDeviceMeta: Job? = null
        var priorCatalog: Job? = null
        var priorWebappDoc: Job? = null
        var priorAuth: Job? = null
        var priorCompanion: BridgethingCompanion? = null
        stateLock.withLock {
            priorEvents = eventsJob
            priorOta = otaJob
            priorDeviceMeta = deviceMetaJob
            priorCatalog = catalogJob
            priorWebappDoc = webappDocJob
            priorAuth = authJob
            priorCompanion = companion
            companion = null
            eventsJob = null
            otaJob = null
            deviceMetaJob = null
            catalogJob = null
            webappDocJob = null
            authJob = null
        }
        priorEvents?.cancel()
        priorOta?.cancel()
        priorDeviceMeta?.cancel()
        priorCatalog?.cancel()
        priorWebappDoc?.cancel()
        priorAuth?.cancel()
        // detach UI observers but leave the companion running in the foreground service.
        priorCompanion?.setNowPlayingObserver(null)
        priorCompanion?.setAncsAuthStateObserver(null)
        priorCompanion?.setLogObserver(null)
        peers.clear()
        lastNowPlaying = null
        emitNowPlaying(null)
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
            glue.setAuthObserver { state -> handleGlueAuthState(state) }
            glue.setServiceHealthObserver { health -> emitServiceHealth(toRnServiceHealth(health)) }
            c.setActive(glue)
        } catch (e: Throwable) {
            emitAuth(authState(BridgethingAuthKind.FAILED, message = e.message ?: e.toString()))
            throw e
        }
    }

    private fun handleGlueAuthState(state: GlueAuthState) {
        when (state) {
            is GlueAuthState.Pending -> emitAuth(
                authState(
                    BridgethingAuthKind.PENDING,
                    userCode = state.prompt?.userCode,
                    verificationUrl = state.prompt?.verificationUrl,
                    verificationUrlComplete = state.prompt?.verificationUrlComplete,
                ),
            )
            is GlueAuthState.Authenticated -> {
                activeRegistration?.let {
                    emitProvider(BridgethingProviderInfo(id = it.id, displayName = it.displayName, available = it.available))
                }
                emitAuth(authState(BridgethingAuthKind.AUTHENTICATED))
            }
            is GlueAuthState.Failed -> emitAuth(authState(BridgethingAuthKind.FAILED, message = state.reason))
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
        emitServiceHealth(toRnServiceHealth(GlueServiceHealth.Ok))
        emitAuth(idleState())
    }

    override suspend fun currentProvider(): BridgethingProviderInfo? {
        val glue = stateLock.withLock { companion }?.current() ?: return null
        return registry.firstOrNull { it.id == glue.name }?.let {
            BridgethingProviderInfo(id = it.id, displayName = it.displayName, available = it.available)
        }
    }

    override suspend fun snapshot(): BridgethingSessionSnapshot {
        val c = stateLock.withLock { companion }
        val glue = c?.current()
        val provider = glue?.let { g ->
            registry.firstOrNull { it.id == g.name }?.let {
                BridgethingProviderInfo(id = it.id, displayName = it.displayName, available = it.available)
            }
        }
        val ancs = toRnAncsAuthStatus(c?.currentAncsAuthState() ?: AncsAuthState.Unknown)
        val deviceMetaEntries = mutableListOf<BridgethingDeviceMetaEntry>()
        if (c != null) {
            for (id in peers.keys) {
                val meta = c.ota.meta(id) ?: continue
                deviceMetaEntries.add(BridgethingDeviceMetaEntry(deviceId = id, meta = toRnDeviceMeta(meta)))
            }
        }
        return BridgethingSessionSnapshot(
            hostInfo = rnHostInfo(),
            provider = provider,
            authState = lastAuthState,
            serviceHealth = lastServiceHealth,
            peers = peers.values.toTypedArray(),
            ancsAuthStatus = ancs,
            nowPlaying = lastNowPlaying,
            deviceMeta = deviceMetaEntries.toTypedArray(),
            capabilityFlags = loadCapabilityFlags(),
            otaPollConfig = loadOtaPollConfig(),
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
        // the bundle write is IO, but startActivity has to run on the main thread
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
        val debug = c?.current()?.debugState() ?: GlueDebugState()
        val ancs = toRnAncsAuthStatus(c?.currentAncsAuthState() ?: AncsAuthState.Unknown)
        return BridgethingCompanionDebug(
            authorityPlaybackHeld = debug.authorityPlaybackHeld,
            authorityMetadataHeld = debug.authorityMetadataHeld,
            ancsAuthStatus = ancs,
        )
    }

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
        val idBytes = value.id ?: return null
        return BridgethingActiveWebapp(id = uuidFromBytes(idBytes).toString(), name = value.name)
    }

    override suspend fun installWebapp(deviceId: String, sourceUri: String): BridgethingWebappInfo {
        val c = requireCompanion(deviceId)
        val (archive, isTemporary) = resolveArchive(sourceUri)
        try {
            return when (val result = c.ota.installWebapp(c.gateway, deviceId, archive)) {
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

    override suspend fun catalogSources(): Array<String> =
        stateLock.withLock { companion }?.catalog?.sources()?.toTypedArray() ?: emptyArray()

    override suspend fun addCatalogSource(url: String) {
        stateLock.withLock { companion }?.catalog?.addSource(url)
    }

    override suspend fun removeCatalogSource(url: String) {
        stateLock.withLock { companion }?.catalog?.removeSource(url)
    }

    override suspend fun refreshCatalog() {
        stateLock.withLock { companion }?.catalog?.refresh()
    }

    override suspend fun availableCatalogApps(deviceId: String): String {
        val catalog = stateLock.withLock { companion }?.catalog ?: return "[]"
        val listings = catalog.availableApps(deviceId)
        return catalogJson.encodeToString(ListSerializer(CatalogAppListing.serializer()), listings)
    }

    override suspend fun checkForCatalogUpdates(deviceId: String): String {
        val catalog = stateLock.withLock { companion }?.catalog ?: return "[]"
        val updates = catalog.checkForUpdates(deviceId)
        return catalogJson.encodeToString(ListSerializer(CatalogAppUpdate.serializer()), updates)
    }

    override suspend fun installCatalogApp(deviceId: String, appId: String, version: String, sourceUrl: String): BridgethingWebappInfo {
        val c = requireCompanion(deviceId)
        return when (val result = c.catalog.install(deviceId, appId, version, sourceUrl)) {
            is WebappInstallResult.Installed -> {
                emitWebappsChanged(deviceId)
                toRnWebappInfo(result.info)
            }
            is WebappInstallResult.Failed -> throw IllegalStateException("install failed: ${result.reason}")
        }
    }

    override suspend fun setCatalogPollConfig(config: BridgethingCatalogPollConfig?) {
        val catalog = stateLock.withLock { companion }?.catalog ?: return
        if (config == null) {
            catalog.setPollConfig(null)
        } else {
            catalog.setPollConfig(
                KCatalogPollConfig(
                    intervalSeconds = config.intervalSeconds.toLong().coerceAtLeast(60L),
                    autoInstall = config.autoInstall,
                )
            )
        }
    }

    override suspend fun reconnectPeer(deviceId: String) {
        stateLock.withLock { companion }?.gateway?.reconnect(deviceId)
    }

    override suspend fun deviceSetNickname(deviceId: String, nickname: String) {
        val c = requireCompanion(deviceId)
        when (val result = c.gateway.system.deviceSetNickname(deviceId, DeviceSetNickname(nickname))) {
            // daemon broadcasts DeviceNicknameChanged; meta lands via ota.metaChanged
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
            )
        }
        return BridgethingCapabilityFlags(
            geo = prefs.getBoolean("caps.geo", true),
            notifications = prefs.getBoolean("caps.notifications", true),
            netFetch = prefs.getBoolean("caps.netFetch", true),
            netWs = prefs.getBoolean("caps.netWs", true),
            audioTts = prefs.getBoolean("caps.audioTts", true),
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

    override fun setOnProviderChanged(callback: (BridgethingProviderInfo?) -> Unit) { onProviderChanged = callback }
    override fun setOnAuthStateChanged(callback: (BridgethingAuthState) -> Unit) { onAuthStateChanged = callback }
    override fun setOnServiceHealthChanged(callback: (BridgethingServiceHealth) -> Unit) { onServiceHealthChanged = callback }
    override fun setOnPeerConnected(callback: (BridgethingSessionPeer) -> Unit) { onPeerConnected = callback }
    override fun setOnPeerDisconnected(callback: (String) -> Unit) { onPeerDisconnected = callback }
    override fun setOnPeerLinkFailed(callback: (BridgethingSessionPeer) -> Unit) { onPeerLinkFailed = callback }
    override fun setOnNowPlayingChanged(callback: (BridgethingNowPlaying?) -> Unit) { onNowPlayingChanged = callback }
    override fun setOnAncsAuthStatusChanged(callback: (BridgethingAncsAuthStatus) -> Unit) { onAncsAuthStatusChanged = callback }
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

    override fun setOnWebappsChanged(callback: (String) -> Unit) { onWebappsChanged = callback }
    override fun setOnWebappDocChanged(callback: (String, String, String, String?) -> Unit) { onWebappDocChanged = callback }
    override fun setOnDeviceMetaChanged(callback: (String, BridgethingDeviceMeta) -> Unit) { onDeviceMetaChanged = callback }
    override fun setOnOtaEvent(callback: (BridgethingOtaEvent) -> Unit) { onOtaEvent = callback }
    override fun setOnCatalogEvent(callback: (BridgethingCatalogEvent) -> Unit) { onCatalogEvent = callback }

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

    private fun uuidFromBytes(bytes: ByteArray): UUID {
        val bb = ByteBuffer.wrap(bytes)
        return UUID(bb.long, bb.long)
    }

    private fun emitWebappsChanged(deviceId: String) {
        if (CompanionHolder.foreground) onWebappsChanged?.invoke(deviceId)
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
        description = info.description,
        iconHash = info.iconHash,
        settingsHash = info.settingsHash,
        config = info.config.map(::toRnConfigField).toTypedArray(),
        permissions = info.permissions.toTypedArray(),
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
        // glues tick once a second on position; skip the JS bridge hop when nothing visible changed.
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

    private fun emitProvider(info: BridgethingProviderInfo?) { if (CompanionHolder.foreground) onProviderChanged?.invoke(info) }
    private fun emitAuth(state: BridgethingAuthState) { lastAuthState = state; if (CompanionHolder.foreground) onAuthStateChanged?.invoke(state) }
    private fun emitServiceHealth(health: BridgethingServiceHealth) {
        lastServiceHealth = health
        if (CompanionHolder.foreground) onServiceHealthChanged?.invoke(health)
    }

    private fun toRnServiceHealth(health: GlueServiceHealth): BridgethingServiceHealth = when (health) {
        is GlueServiceHealth.Ok -> BridgethingServiceHealth(BridgethingServiceHealthKind.OK, null)
        is GlueServiceHealth.RateLimited ->
            BridgethingServiceHealth(BridgethingServiceHealthKind.RATELIMITED, health.retryAfterSeconds.toDouble())
        is GlueServiceHealth.Unreachable -> BridgethingServiceHealth(BridgethingServiceHealthKind.UNREACHABLE, null)
    }
    private fun emitNowPlaying(np: BridgethingNowPlaying?) { if (CompanionHolder.foreground) onNowPlayingChanged?.invoke(np) }
    private fun emitAncsAuthStatus(status: BridgethingAncsAuthStatus) { if (CompanionHolder.foreground) onAncsAuthStatusChanged?.invoke(status) }

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

    private fun toRnOtaEvent(ev: OtaPollEvent): BridgethingOtaEvent = when (ev) {
        is OtaPollEvent.ManifestPolled -> makeOtaEvent(
            kind = BridgethingOtaEventKind.MANIFESTPOLLED, updatedAt = ev.updatedAt,
        )
        is OtaPollEvent.ManifestPollFailed -> makeOtaEvent(
            kind = BridgethingOtaEventKind.MANIFESTPOLLFAILED, reason = ev.reason,
        )
        is OtaPollEvent.UpdateAvailable -> makeOtaEvent(
            kind = BridgethingOtaEventKind.UPDATEAVAILABLE,
            deviceId = ev.deviceId, toVersion = ev.release,
            releaseVersion = ev.release, daemonVersion = ev.daemonVersion, imageVersion = ev.imageVersion,
        )
        is OtaPollEvent.Planned -> makeOtaEvent(
            kind = BridgethingOtaEventKind.PLANNED,
            deviceId = ev.deviceId, otaKind = toRnOtaKind(ev.kind),
            releaseVersion = ev.release, daemonVersion = ev.daemonVersion, imageVersion = ev.imageVersion,
            steps = ev.steps.map { BridgethingOtaStep(it.id.toDouble(), toRnStepKind(it.kind), it.label, it.bytes.toDouble()) }.toTypedArray(),
        )
        is OtaPollEvent.Progress -> snapshotToEvent(ev.deviceId, toRnOtaKind(ev.kind), ev.stepId.toDouble(), ev.snapshot)
        is OtaPollEvent.Updated -> makeOtaEvent(
            kind = BridgethingOtaEventKind.UPDATED,
            deviceId = ev.deviceId, otaKind = toRnOtaKind(ev.kind), toVersion = ev.version,
        )
        is OtaPollEvent.Failed -> makeOtaEvent(
            kind = BridgethingOtaEventKind.FAILED,
            deviceId = ev.deviceId, otaKind = toRnOtaKind(ev.kind), reason = ev.reason,
        )
    }

    private fun bytePercent(n: Long, d: Long): Double =
        if (d <= 0L) 0.0 else minOf(100.0, n.toDouble() * 100 / d.toDouble())

    private fun snapshotToEvent(
        deviceId: String,
        otaKind: BridgethingOtaKind,
        stepId: Double,
        snapshot: OtaPhaseSnapshot,
    ): BridgethingOtaEvent = when (snapshot) {
        OtaPhaseSnapshot.Idle -> makeOtaEvent(
            kind = BridgethingOtaEventKind.PROGRESS, deviceId = deviceId, otaKind = otaKind, stepId = stepId,
            phase = BridgethingOtaPhase.IDLE, percent = 0.0,
        )
        is OtaPhaseSnapshot.Downloading -> makeOtaEvent(
            kind = BridgethingOtaEventKind.PROGRESS, deviceId = deviceId, otaKind = otaKind, stepId = stepId,
            phase = BridgethingOtaPhase.DOWNLOADING, percent = bytePercent(snapshot.received, snapshot.total),
            stageAsset = snapshot.asset, stageReceived = snapshot.received.toDouble(),
            stageTotal = snapshot.total.toDouble(), stageRatePerSec = snapshot.ratePerSec,
        )
        is OtaPhaseSnapshot.Streaming -> makeOtaEvent(
            kind = BridgethingOtaEventKind.PROGRESS, deviceId = deviceId, otaKind = otaKind, stepId = stepId,
            phase = BridgethingOtaPhase.STREAMING, percent = bytePercent(snapshot.sent, snapshot.total),
            stageAsset = snapshot.asset, stageReceived = snapshot.sent.toDouble(), stageTotal = snapshot.total.toDouble(),
            stageRatePerSec = snapshot.ratePerSec, stageEtaSeconds = snapshot.etaSeconds,
        )
        is OtaPhaseSnapshot.Applying -> {
            val rnPhase = when (snapshot.phase) {
                OtaPhase.Streaming -> BridgethingOtaPhase.STREAMING
                OtaPhase.Verifying -> BridgethingOtaPhase.VERIFYING
                OtaPhase.Writing -> BridgethingOtaPhase.WRITING
                OtaPhase.Confirming -> BridgethingOtaPhase.CONFIRMING
                OtaPhase.Reboot -> BridgethingOtaPhase.REBOOT
            }
            makeOtaEvent(
                kind = BridgethingOtaEventKind.PROGRESS, deviceId = deviceId, otaKind = otaKind, stepId = stepId,
                phase = rnPhase, percent = snapshot.writePercent.toDouble(), dwlPercent = snapshot.dwlPercent.toDouble(),
                stageReceived = snapshot.dwlBytes.takeIf { snapshot.dwlPercent < 100 && it > 0 }?.toDouble(),
            )
        }
        OtaPhaseSnapshot.Staged -> makeOtaEvent(
            kind = BridgethingOtaEventKind.PROGRESS, deviceId = deviceId, otaKind = otaKind, stepId = stepId,
            phase = BridgethingOtaPhase.WRITING, percent = 100.0,
        )
        OtaPhaseSnapshot.Completed -> makeOtaEvent(
            kind = BridgethingOtaEventKind.PROGRESS, deviceId = deviceId, otaKind = otaKind, stepId = stepId,
            phase = BridgethingOtaPhase.COMPLETED, percent = 100.0,
        )
        is OtaPhaseSnapshot.Failed -> makeOtaEvent(
            kind = BridgethingOtaEventKind.PROGRESS, reason = snapshot.reason, deviceId = deviceId,
            otaKind = otaKind, stepId = stepId, phase = BridgethingOtaPhase.FAILED, percent = 0.0,
        )
    }

    private fun toRnOtaKind(kind: OtaKind): BridgethingOtaKind = when (kind) {
        OtaKind.Image -> BridgethingOtaKind.IMAGE
        OtaKind.Daemon -> BridgethingOtaKind.DAEMON
        OtaKind.BuiltinWebapp -> BridgethingOtaKind.BUILTINWEBAPP
        // installed-webapp installs return WebappInfo directly and never emit OTA events.
        OtaKind.InstalledWebapp -> error("installed-webapp does not flow through OTA events")
    }

    private fun makeOtaEvent(
        kind: BridgethingOtaEventKind,
        updatedAt: String? = null,
        reason: String? = null,
        deviceId: String? = null,
        otaKind: BridgethingOtaKind? = null,
        fromVersion: String? = null,
        toVersion: String? = null,
        releaseVersion: String? = null,
        daemonVersion: String? = null,
        imageVersion: String? = null,
        steps: Array<BridgethingOtaStep>? = null,
        stepId: Double? = null,
        phase: BridgethingOtaPhase? = null,
        percent: Double? = null,
        dwlPercent: Double? = null,
        stageAsset: String? = null,
        stageReceived: Double? = null,
        stageTotal: Double? = null,
        stageRatePerSec: Double? = null,
        stageEtaSeconds: Double? = null,
    ): BridgethingOtaEvent = BridgethingOtaEvent(
        kind = kind,
        updatedAt = updatedAt,
        reason = reason,
        deviceId = deviceId,
        otaKind = otaKind,
        fromVersion = fromVersion,
        toVersion = toVersion,
        releaseVersion = releaseVersion,
        daemonVersion = daemonVersion,
        imageVersion = imageVersion,
        steps = steps,
        stepId = stepId,
        phase = phase,
        percent = percent,
        dwlPercent = dwlPercent,
        stageAsset = stageAsset,
        stageReceived = stageReceived,
        stageTotal = stageTotal,
        stageRatePerSec = stageRatePerSec,
        stageEtaSeconds = stageEtaSeconds,
    )

    private fun toRnStepKind(k: OtaStepKind): BridgethingOtaStepKind = when (k) {
        OtaStepKind.DOWNLOAD -> BridgethingOtaStepKind.DOWNLOAD
        OtaStepKind.STREAM -> BridgethingOtaStepKind.STREAM
        OtaStepKind.APPLY -> BridgethingOtaStepKind.APPLY
        OtaStepKind.REBOOT -> BridgethingOtaStepKind.REBOOT
    }

    private val catalogJson = Json { ignoreUnknownKeys = true; explicitNulls = false }

    private fun toRnCatalogEvent(ev: CatalogEvent): BridgethingCatalogEvent = when (ev) {
        is CatalogEvent.Refreshed -> makeCatalogEvent(
            kind = BridgethingCatalogEventKind.REFRESHED,
            sourceCount = ev.sourceCount.toDouble(), appCount = ev.appCount.toDouble(),
        )
        is CatalogEvent.SourceFailed -> makeCatalogEvent(
            kind = BridgethingCatalogEventKind.SOURCEFAILED, url = ev.url, reason = ev.reason,
        )
        is CatalogEvent.UpdateAvailable -> makeCatalogEvent(
            kind = BridgethingCatalogEventKind.UPDATEAVAILABLE,
            deviceId = ev.deviceId, appId = ev.update.appId, name = ev.update.name,
            url = ev.update.sourceUrl, fromVersion = ev.update.installedVersion, toVersion = ev.update.target.version,
        )
        is CatalogEvent.Installed -> makeCatalogEvent(
            kind = BridgethingCatalogEventKind.INSTALLED,
            deviceId = ev.deviceId, appId = ev.appId, version = ev.version,
        )
        is CatalogEvent.InstallFailed -> makeCatalogEvent(
            kind = BridgethingCatalogEventKind.INSTALLFAILED,
            deviceId = ev.deviceId, appId = ev.appId, reason = ev.reason,
        )
    }

    private fun makeCatalogEvent(
        kind: BridgethingCatalogEventKind,
        sourceCount: Double? = null,
        appCount: Double? = null,
        url: String? = null,
        reason: String? = null,
        deviceId: String? = null,
        appId: String? = null,
        name: String? = null,
        fromVersion: String? = null,
        toVersion: String? = null,
        version: String? = null,
    ): BridgethingCatalogEvent = BridgethingCatalogEvent(
        kind = kind,
        sourceCount = sourceCount,
        appCount = appCount,
        url = url,
        reason = reason,
        deviceId = deviceId,
        appId = appId,
        name = name,
        fromVersion = fromVersion,
        toVersion = toVersion,
        version = version,
    )
}
