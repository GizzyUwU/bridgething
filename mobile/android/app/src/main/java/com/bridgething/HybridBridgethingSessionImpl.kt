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
import com.margelo.nitro.bridgething.session.BridgethingBtDevice
import com.margelo.nitro.bridgething.session.BridgethingCapabilityFlags
import com.margelo.nitro.bridgething.session.BridgethingCompanionDebug
import com.margelo.nitro.bridgething.session.BridgethingConfigEntry
import com.margelo.nitro.bridgething.session.BridgethingConfigField
import com.margelo.nitro.bridgething.session.BridgethingConfigKind
import com.margelo.nitro.bridgething.session.BridgethingDeviceMeta
import com.margelo.nitro.bridgething.session.BridgethingDeviceMetaEntry
import com.margelo.nitro.bridgething.session.BridgethingDiagDirection
import com.margelo.nitro.bridgething.session.BridgethingDiagEntry
import com.margelo.nitro.bridgething.session.BridgethingDiagFrameKind
import com.margelo.nitro.bridgething.session.BridgethingDiagKind
import com.margelo.nitro.bridgething.session.BridgethingHostInfo
import com.margelo.nitro.bridgething.session.BridgethingSessionSnapshot
import com.margelo.nitro.bridgething.session.BridgethingNowPlaying
import com.margelo.nitro.bridgething.session.BridgethingNowPlayingPlayback
import com.margelo.nitro.bridgething.session.BridgethingNowPlayingTrack
import com.margelo.nitro.bridgething.session.BridgethingOtaChannelInfo
import com.margelo.nitro.bridgething.session.BridgethingOtaEvent
import com.margelo.nitro.bridgething.session.BridgethingOtaEventKind
import com.margelo.nitro.bridgething.session.BridgethingOtaKind
import com.margelo.nitro.bridgething.session.BridgethingOtaManifest
import com.margelo.nitro.bridgething.session.BridgethingOtaPhase
import com.margelo.nitro.bridgething.session.BridgethingOtaPollConfig
import com.margelo.nitro.bridgething.session.BridgethingOtaRelease
import com.margelo.nitro.bridgething.session.BridgethingPeerLinkStatus
import com.margelo.nitro.bridgething.session.BridgethingProviderInfo
import com.margelo.nitro.bridgething.session.BridgethingRepeatMode
import com.margelo.nitro.bridgething.session.BridgethingServiceHealth
import com.margelo.nitro.bridgething.session.BridgethingServiceHealthKind
import com.margelo.nitro.bridgething.session.BridgethingSessionPeer
import com.margelo.nitro.bridgething.session.BridgethingSpotifyAuthConfig
import com.margelo.nitro.bridgething.session.BridgethingWebappIcon
import com.margelo.nitro.bridgething.session.BridgethingWebappInfo
import com.margelo.nitro.bridgething.session.BridgethingWebappRole
import com.margelo.nitro.bridgething.session.BridgethingWebappSource
import dev.bridgething.companion.AncsSetupKind
import dev.bridgething.companion.BridgethingCompanion
import dev.bridgething.companion.BridgethingCompanionVersion
import dev.bridgething.companion.CompanionCapabilityFlags
import dev.bridgething.companion.CompanionLogLevel
import dev.bridgething.companion.HostInfo
import dev.bridgething.companion.OtaCompositeVersion
import dev.bridgething.companion.OtaDiscoverManifest
import dev.bridgething.companion.OtaPhaseSnapshot
import dev.bridgething.companion.OtaPollConfig as KOtaPollConfig
import dev.bridgething.companion.OtaPollEvent
import dev.bridgething.gateway.DiagRecord
import dev.bridgething.gateway.DiagnosticsBuffer
import dev.bridgething.gateway.GatewayEvent
import dev.bridgething.gateway.RequestResult
import dev.bridgething.gateway.device
import dev.bridgething.gateway.webapp
import dev.bridgething.glue.BridgethingGlue
import dev.bridgething.glue.GlueAuthState
import dev.bridgething.glue.GlueDebugState
import dev.bridgething.glue.GlueNowPlaying
import dev.bridgething.schema.BridgeThingMeta
import dev.bridgething.glue.GlueServiceHealth
import dev.bridgething.lyrics.LrclibResolver
import dev.bridgething.lyrics.LyricsResolver
import dev.bridgething.schema.AncsAuthState
import dev.bridgething.schema.ConfigField
import dev.bridgething.schema.OtaKind
import dev.bridgething.schema.OtaPhase
import dev.bridgething.schema.Priority
import dev.bridgething.schema.RepeatMode
import dev.bridgething.schema.WebappConfigDelete
import dev.bridgething.schema.WebappConfigList
import dev.bridgething.schema.WebappConfigSet
import dev.bridgething.schema.WebappError
import dev.bridgething.schema.WebappIcon
import dev.bridgething.schema.WebappInfo
import dev.bridgething.schema.WebappInstallBegin
import dev.bridgething.schema.WebappInstallChunk
import dev.bridgething.schema.WebappRole
import dev.bridgething.schema.WebappSource
import dev.bridgething.schema.WebappSwitchTo
import dev.bridgething.schema.WebappUninstall
import java.io.File
import java.io.RandomAccessFile
import java.net.HttpURLConnection
import java.net.URI
import java.net.URL
import java.nio.ByteBuffer
import java.security.MessageDigest
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.selects.select
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

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
    }

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val stateLock = Mutex()
    private var companion: BridgethingCompanion? = null
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
    private var onDeviceMetaChanged: ((String, BridgethingDeviceMeta) -> Unit)? = null

    @Volatile
    private var onOtaEvent: ((BridgethingOtaEvent) -> Unit)? = null

    @Volatile
    private var onDiagEntry: ((BridgethingDiagEntry) -> Unit)? = null
    private var diagJob: Job? = null

    @Volatile
    private var lastAuthState: BridgethingAuthState = idleState()

    @Volatile
    private var lastServiceHealth: BridgethingServiceHealth = toRnServiceHealth(GlueServiceHealth.Ok)

    private val prefs by lazy {
        context.applicationContext.getSharedPreferences("bridgething.session", Context.MODE_PRIVATE)
    }

    @Volatile
    private var logStreamingDesired: Boolean = false

    override suspend fun start() {
        // the foreground service owns the companion's lifetime; the UI just borrows the reference.
        val c = CompanionHolder.ensureStarted(context)
        val firstAttach = stateLock.withLock {
            if (companion != null) return@withLock false
            companion = c
            true
        }
        if (!firstAttach) return

        c.setNowPlayingObserver { np -> handleNowPlaying(np) }
        c.setAncsAuthStateObserver { state -> emitAncsAuthStatus(toRnAncsAuthStatus(state)) }
        if (logStreamingDesired) {
            c.setLogObserver { level, message -> onLog?.invoke(level.raw, message) }
        }
        eventsJob = scope.launch { c.gateway.events.collect { event -> handleGatewayEvent(event) } }
        otaJob = scope.launch { c.ota.events.collect { ev -> onOtaEvent?.invoke(toRnOtaEvent(ev)) } }
        diagJob = scope.launch { DiagnosticsBuffer.stream.collect { rec -> onDiagEntry?.invoke(toRnDiagEntry(rec)) } }

        runCatching { applyCapabilityFlags(loadCapabilityFlags()) }
        runCatching { applyOtaPollConfig(loadOtaPollConfig()) }

        // wake-from-cold on presence + keep the link alive while a device is set up.
        CompanionDevicePicker.startObservingPresence(context)
        if (CompanionDevicePicker.associations(context.applicationContext).isNotEmpty()) {
            BridgethingConnectionService.start(context)
        }

        // restore the last signed-in provider so the app opens already authenticated.
        registry.firstOrNull { it.available && it.hasCredentials() }?.let {
            runCatching { setActiveProvider(it.id) }
        }
    }

    override suspend fun stop() {
        var priorEvents: Job? = null
        var priorOta: Job? = null
        var priorAuth: Job? = null
        var priorDiag: Job? = null
        var priorCompanion: BridgethingCompanion? = null
        stateLock.withLock {
            priorEvents = eventsJob
            priorOta = otaJob
            priorAuth = authJob
            priorDiag = diagJob
            priorCompanion = companion
            companion = null
            eventsJob = null
            otaJob = null
            authJob = null
            diagJob = null
        }
        priorEvents?.cancel()
        priorOta?.cancel()
        priorAuth?.cancel()
        priorDiag?.cancel()
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

    override suspend fun spotifyAuthConfig(): BridgethingSpotifyAuthConfig = BridgethingApp.spotifyAuthConfig()

    override suspend fun completeSpotifySignIn(accessToken: String, refreshToken: String, usesDealer: Boolean) {
        BridgethingApp.persistSpotifyTokens(context, accessToken, refreshToken)
        setActiveProvider(BridgethingApp.SPOTIFY_PROVIDER_ID)
    }

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

    override suspend fun diagnosticsSnapshot(limit: Double): Array<BridgethingDiagEntry> =
        DiagnosticsBuffer.tail(limit.toInt()).map(::toRnDiagEntry).toTypedArray()

    override suspend fun companionDebug(): BridgethingCompanionDebug {
        val c = stateLock.withLock { companion }
        val debug = c?.current()?.debugState() ?: GlueDebugState()
        val ancs = toRnAncsAuthStatus(c?.currentAncsAuthState() ?: AncsAuthState.Unknown)
        return BridgethingCompanionDebug(
            authorityPlaybackHeld = debug.authorityPlaybackHeld,
            authorityMetadataHeld = debug.authorityMetadataHeld,
            baselinePollActive = debug.baselinePollActive,
            hintFetchActive = debug.hintFetchActive,
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
            val size = archive.length()
            require(size in 1..UInt.MAX_VALUE.toLong()) { "invalid archive" }
            val installId = sha256HexOfFile(archive)
            val ack = unwrapWebapp(
                c.gateway.webapp.installBegin(deviceId, WebappInstallBegin(installId, installId, size.toUInt())),
                "installBegin",
            )

            // subscribe before the last chunk to avoid racing the daemon's installed broadcast.
            val installed = scope.async {
                c.gateway.webapp.webappInstalled.first { it.first == deviceId }.second
            }
            val failed = scope.async {
                c.gateway.webapp.webappInstallFailed
                    .first { it.first == deviceId && it.second.installId == installId }.second.error
            }
            try {
                streamInstallChunks(c, deviceId, installId, archive, size, ack.resumeFromOffset.toLong())
                val info = withTimeoutOrNull(60_000L) {
                    select<WebappInfo> {
                        installed.onAwait { it }
                        failed.onAwait { throw IllegalStateException("install failed: $it") }
                    }
                } ?: throw IllegalStateException("install timed out")
                emitWebappsChanged(deviceId)
                return toRnWebappInfo(info)
            } finally {
                installed.cancel()
                failed.cancel()
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
        return when (val result = c.gateway.webapp.icon(deviceId, WebappIcon(uuid))) {
            is RequestResult.Ok -> {
                // svg ships inline as markup for <SvgXml>; raster goes through the file cache for <Image>.
                if (result.response.mime == "image/svg+xml") {
                    BridgethingWebappIcon(
                        fileUri = null,
                        svg = result.response.bytes.toString(Charsets.UTF_8),
                        mime = result.response.mime,
                    )
                } else {
                    val file = writeIconToCache(deviceId, id, result.response.mime, result.response.bytes)
                    BridgethingWebappIcon(fileUri = Uri.fromFile(file).toString(), svg = null, mime = result.response.mime)
                }
            }
            is RequestResult.DomainErr ->
                if (result.error is WebappError.IconNotAvailable) null
                else throw IllegalStateException("webappIcon: ${result.error}")
            is RequestResult.ProtocolErr -> throw IllegalStateException("webappIcon: ${result.error}")
        }
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
                    channel = config.channel,
                    intervalSeconds = config.intervalSeconds.toLong().coerceAtLeast(60L),
                    cacheDirectory = context.cacheDir,
                    autoPush = config.autoPush,
                )
            )
        }
    }

    override suspend fun checkForOtaUpdate(channel: String, rootUrl: String?) {
        stateLock.withLock { companion }?.ota?.checkNow(channel, otaRootUrl(rootUrl))
    }

    override suspend fun fetchOtaManifest(rootUrl: String?): BridgethingOtaManifest {
        val ota = stateLock.withLock { companion }?.ota
            ?: return BridgethingOtaManifest(updatedAt = "", channels = emptyArray())
        return toRnOtaManifest(ota.discoverManifest(otaRootUrl(rootUrl)))
    }

    override suspend fun applyOtaUpdate(deviceId: String, channel: String, version: String, rootUrl: String?) {
        stateLock.withLock { companion }?.ota?.applyVersion(deviceId, channel, version, otaRootUrl(rootUrl))
    }

    override suspend fun reconnectPeer(deviceId: String) {
        stateLock.withLock { companion }?.gateway?.reconnect(deviceId)
    }

    private fun otaRootUrl(raw: String?): String = raw ?: "https://ota.bridgething.com"

    private fun toRnOtaManifest(m: OtaDiscoverManifest): BridgethingOtaManifest {
        val channels = m.channels.values.map { ch ->
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
        if (!prefs.getBoolean("ota.configured", false)) return null
        val root = prefs.getString("ota.rootUrl", null)
        return BridgethingOtaPollConfig(
            channel = prefs.getString("ota.channel", "stable") ?: "stable",
            intervalSeconds = prefs.getLong("ota.intervalSeconds", 21600L).toDouble(),
            autoPush = prefs.getBoolean("ota.autoPush", false),
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
            .putString("ota.channel", config.channel)
            .putLong("ota.intervalSeconds", config.intervalSeconds.toLong())
            .putBoolean("ota.autoPush", config.autoPush)
            .putString("ota.rootUrl", config.rootUrl)
            .apply()
    }

    // --- diagnostics record conversion ---

    private fun toRnDiagEntry(r: DiagRecord): BridgethingDiagEntry = BridgethingDiagEntry(
        seq = r.seq.toDouble(),
        ts = r.timestampMs,
        kind = when (r.kind) {
            DiagRecord.Kind.FRAME -> BridgethingDiagKind.FRAME
            DiagRecord.Kind.LOG -> BridgethingDiagKind.LOG
            DiagRecord.Kind.BREADCRUMB -> BridgethingDiagKind.BREADCRUMB
        },
        deviceId = r.deviceId,
        direction = r.direction?.let {
            when (it) {
                DiagRecord.Direction.OUTBOUND -> BridgethingDiagDirection.OUTBOUND
                DiagRecord.Direction.INBOUND -> BridgethingDiagDirection.INBOUND
            }
        },
        frameKind = r.frameKind?.let {
            when (it) {
                DiagRecord.FrameKind.REQUEST -> BridgethingDiagFrameKind.REQUEST
                DiagRecord.FrameKind.RESPONSE -> BridgethingDiagFrameKind.RESPONSE
                DiagRecord.FrameKind.EVENT -> BridgethingDiagFrameKind.EVENT
                DiagRecord.FrameKind.COMMAND -> BridgethingDiagFrameKind.COMMAND
            }
        },
        surface = r.surface,
        byteSize = r.byteSize?.toDouble(),
        requestId = r.requestId,
        latencyMs = r.latencyMs,
        level = r.level,
        target = r.target,
        message = r.message,
        category = r.category,
        detail = r.detail,
        fields = r.fields?.map { BridgethingConfigEntry(it.first, it.second) }?.toTypedArray(),
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
        // observe the new association, keep it alive, and connect now so the peer appears
        // without an app restart.
        CompanionDevicePicker.startObservingPresence(context)
        BridgethingConnectionService.start(context)
        CompanionHolder.reconnectAssociated(context)
        return picked
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
        if (enabled) {
            c.setLogObserver { level, msg -> onLog?.invoke(level.raw, msg) }
        } else {
            c.setLogObserver(null)
        }
    }

    override fun setOnWebappsChanged(callback: (String) -> Unit) { onWebappsChanged = callback }
    override fun setOnDeviceMetaChanged(callback: (String, BridgethingDeviceMeta) -> Unit) { onDeviceMetaChanged = callback }
    override fun setOnOtaEvent(callback: (BridgethingOtaEvent) -> Unit) { onOtaEvent = callback }
    override fun setOnDiagEntry(callback: (BridgethingDiagEntry) -> Unit) { onDiagEntry = callback }

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
        onWebappsChanged?.invoke(deviceId)
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

    private fun sha256HexOfFile(file: File): String {
        val md = MessageDigest.getInstance("SHA-256")
        file.inputStream().use { input ->
            val buf = ByteArray(1024 * 1024)
            while (true) {
                val n = input.read(buf)
                if (n <= 0) break
                md.update(buf, 0, n)
            }
        }
        return md.digest().joinToString("") { "%02x".format(it) }
    }

    private suspend fun streamInstallChunks(
        c: BridgethingCompanion,
        deviceId: String,
        installId: String,
        archive: File,
        total: Long,
        startOffset: Long,
    ) {
        val device = c.gateway.device(deviceId).webapp
        val chunkSize = 64 * 1024
        RandomAccessFile(archive, "r").use { raf ->
            raf.seek(startOffset)
            var offset = startOffset
            val buf = ByteArray(chunkSize)
            while (offset < total) {
                val want = minOf(chunkSize.toLong(), total - offset).toInt()
                val n = raf.read(buf, 0, want)
                if (n <= 0) throw IllegalStateException("invalid archive")
                val end = offset + n
                device.installChunk(
                    WebappInstallChunk(installId, offset.toUInt(), buf.copyOf(n), end == total),
                    Priority.Bulk,
                )
                offset = end
            }
        }
    }

    private fun writeIconToCache(deviceId: String, id: String, mime: String?, bytes: ByteArray): File {
        val dir = File(context.cacheDir, "bridgething-webapp-icons").apply { mkdirs() }
        val ext = when (mime) {
            "image/png" -> "png"
            "image/jpeg", "image/jpg" -> "jpg"
            "image/webp" -> "webp"
            "image/svg+xml" -> "svg"
            else -> "bin"
        }
        val file = File(dir, "${deviceId.replace("/", "_")}__${id.replace("/", "_")}.$ext")
        file.writeBytes(bytes)
        return file
    }

    private fun toRnWebappInfo(info: WebappInfo): BridgethingWebappInfo = BridgethingWebappInfo(
        id = info.id.toString(),
        name = info.name,
        source = if (info.source == WebappSource.Builtin) BridgethingWebappSource.BUILTIN else BridgethingWebappSource.INSTALLED,
        role = if (info.role == WebappRole.Launcher) BridgethingWebappRole.LAUNCHER else BridgethingWebappRole.STANDARD,
        version = info.version,
        description = info.description,
        iconAvailable = info.iconAvailable,
        iconMime = info.iconMime,
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
    private fun emitAuth(state: BridgethingAuthState) { lastAuthState = state; onAuthStateChanged?.invoke(state) }
    private fun emitServiceHealth(health: BridgethingServiceHealth) {
        lastServiceHealth = health
        onServiceHealthChanged?.invoke(health)
    }

    private fun toRnServiceHealth(health: GlueServiceHealth): BridgethingServiceHealth = when (health) {
        is GlueServiceHealth.Ok -> BridgethingServiceHealth(BridgethingServiceHealthKind.OK, null)
        is GlueServiceHealth.RateLimited ->
            BridgethingServiceHealth(BridgethingServiceHealthKind.RATELIMITED, health.retryAfterSeconds.toDouble())
        is GlueServiceHealth.Unreachable -> BridgethingServiceHealth(BridgethingServiceHealthKind.UNREACHABLE, null)
    }
    private fun emitNowPlaying(np: BridgethingNowPlaying?) { onNowPlayingChanged?.invoke(np) }
    private fun emitAncsAuthStatus(status: BridgethingAncsAuthStatus) { onAncsAuthStatusChanged?.invoke(status) }

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
