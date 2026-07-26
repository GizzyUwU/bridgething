package com.bridgething.companion

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import com.bridgething.gateway.Adapter
import com.bridgething.gateway.AssetRequestHandle
import com.bridgething.gateway.BridgethingGateway
import com.bridgething.gateway.Compression
import com.bridgething.gateway.GatewayEvent
import com.bridgething.gateway.LogStore
import com.bridgething.gateway.LocalLogRelay
import com.bridgething.gateway.LyricsRequestHandle
import com.bridgething.gateway.RequestResult
import com.bridgething.gateway.asset
import com.bridgething.gateway.audio
import com.bridgething.gateway.authority
import com.bridgething.gateway.capabilities
import com.bridgething.gateway.device
import com.bridgething.gateway.lyrics
import com.bridgething.gateway.notifications
import com.bridgething.gateway.system
import com.bridgething.gateway.time
import com.bridgething.gateway.transfer
import com.bridgething.gateway.webapp
import com.bridgething.glue.AssetBytes
import com.bridgething.glue.BridgethingGlue
import com.bridgething.glue.GlueNowPlaying
import com.bridgething.glue.NowPlayingTransport
import com.bridgething.lyrics.Lyrics as DomainLyrics
import com.bridgething.lyrics.LyricsResolver
import com.bridgething.lyrics.TrackIdentity
import com.bridgething.schema.AncsAuthState
import com.bridgething.schema.AssetGotReply
import com.bridgething.schema.Priority
import java.util.UUID
import com.bridgething.schema.AssetNotFoundReply
import com.bridgething.schema.AssetRequest
import com.bridgething.schema.AudioCapabilities
import com.bridgething.schema.AuthorityClaim
import com.bridgething.schema.AuthorityRelease
import com.bridgething.schema.BridgeToGatewayMsgData
import com.bridgething.schema.BridgeToGatewayPlayerMsg
import com.bridgething.schema.CompanionAuthorityScope
import com.bridgething.schema.GatewayCapabilities
import com.bridgething.schema.GatewayInfo
import com.bridgething.schema.KeepaliveAck
import com.bridgething.schema.LogEntry
import com.bridgething.schema.LogLevel
import com.bridgething.schema.LogSource
import com.bridgething.schema.LogsSubscribe
import com.bridgething.schema.LogsUnsubscribe
import com.bridgething.schema.LyricLine
import com.bridgething.schema.Lyrics as WireLyrics
import com.bridgething.schema.LyricsErrorReply
import com.bridgething.schema.LyricsReply
import com.bridgething.schema.LyricsRequest
import com.bridgething.schema.MusicProvider
import com.bridgething.schema.NetworkInfo
import com.bridgething.schema.NetworkKind
import com.bridgething.schema.SurfaceAvailability
import com.bridgething.schema.TimeInfo
import com.bridgething.schema.TransferAbandon
import com.bridgething.schema.TransferBody
import com.bridgething.schema.TransferFragment
import com.bridgething.schema.TransferRef
import com.bridgething.schema.VolumeChanged
import kotlinx.coroutines.CompletableJob
import kotlinx.coroutines.CoroutineName
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.coroutineScope
import com.bridgething.gateway.library
import com.bridgething.gateway.LibraryBrowseRequestHandle
import com.bridgething.gateway.LibraryResolveContextRequestHandle
import com.bridgething.gateway.LibrarySearchRequestHandle
import com.bridgething.gateway.LibraryRecommendationsRequestHandle
import com.bridgething.gateway.LibraryFavoritesListRequestHandle
import com.bridgething.gateway.LibraryFavoritesContainsRequestHandle
import com.bridgething.glue.GlueError
import com.bridgething.schema.BrowseReply
import com.bridgething.schema.SearchReply
import com.bridgething.schema.RecommendationsReply
import com.bridgething.schema.FavoritesListReply
import com.bridgething.schema.FavoritesContainsReply
import com.bridgething.schema.LibraryErrorReply
import com.bridgething.schema.LibraryError
import com.bridgething.schema.LibraryErrorNotSupportedInner
import com.bridgething.schema.WireError
import com.bridgething.schema.LibraryBrowseRequest
import com.bridgething.schema.LibraryResolveContextRequest
import com.bridgething.schema.LibrarySearchRequest
import com.bridgething.schema.LibraryRecommendationsRequest
import com.bridgething.schema.LibraryFavoritesListRequest
import com.bridgething.schema.LibraryFavoritesContainsRequest
import com.bridgething.schema.FavoritesToggle
import com.bridgething.schema.FavoritesSet
import com.bridgething.schema.FavoritesSetMany

public enum class CompanionLogLevel(public val raw: String) {
    Debug("debug"), Info("info"), Warn("warn"), Error("error"),
}

public object BridgethingCompanionVersion {
    public const val LIB: String = "0.1.0"
    public const val LIBBRIDGETHING: String = "0.1.0"
}

public data class HostInfo(
    val appName: String,
    val appVersion: String,
    val osName: String,
    val osVersion: String = "",
    val address: String = "",
    val adapterVersion: String = "",
)

public data class CompanionCapabilityFlags(
    val geo: Boolean = true,
    val notifications: Boolean = true,
    val netFetch: Boolean = true,
    val netWs: Boolean = true,
    val audioTts: Boolean = true,
)

public enum class AncsSetupKind {
    Paired, AlreadyPaired, Cancelled, Unsupported, Failed
}

public data class AncsSetupResult(
    val kind: AncsSetupKind,
    val authState: AncsAuthState,
    val message: String? = null,
)

public class BridgethingCompanion(
    public val context: Context,
    adapter: Adapter,
    private val lyricsResolver: LyricsResolver,
    private val host: HostInfo,
    capabilities: CompanionCapabilityFlags = CompanionCapabilityFlags(),
    httpClient: okhttp3.OkHttpClient = okhttp3.OkHttpClient(),
    geo: GeoSource = GeoController(context = context.applicationContext),
    volume: VolumeSource = VolumeMonitor(context = context.applicationContext),
    audio: AudioBackend = AndroidAudioBackend(context = context.applicationContext),
    notifications: NotificationBackend = NoOpNotificationBackend,
    phone: PhoneBackend = NoOpPhoneBackend,
    mediaSessions: MediaSessionGateway = NoOpMediaSessionGateway,
) {
    public val gateway: BridgethingGateway = BridgethingGateway(adapter)
    public val ota: OtaService = OtaService(httpClient = httpClient)
    private val transferAcks = ota.transferAcks
    private val transferReceiver = TransferReceiver(gateway)
    public val webappResources: WebappResourceService = WebappResourceService(
        cacheDir = context.cacheDir ?: java.io.File(System.getProperty("java.io.tmpdir") ?: "."),
        gateway = gateway,
        receiver = transferReceiver,
    )
    private val netDispatcher = NetDispatcher(client = httpClient)
    private val tunnelDispatcher = TunnelDispatcher()
    private val audioDispatcher = AudioDispatcher(backend = audio)
    private val phoneDispatcher = PhoneDispatcher(backend = phone)
    @Volatile private var notificationsEnabled: Boolean = capabilities.notifications
    private val notificationDispatcher = NotificationDispatcher(notifications) { notificationsEnabled }
    private val geoController: GeoSource = geo
    private val volumeMonitor: VolumeSource = volume
    private val nowPlayingHub = NowPlayingHub(gateway)
    @Volatile private var activeAppBundles: Set<String> = emptySet()
    private val systemMediaSource = SystemMediaSource(mediaSessions, nowPlayingHub) { activeAppBundles }

    private val supervisor: CompletableJob = SupervisorJob()
    private val scope = CoroutineScope(supervisor + Dispatchers.Default + CoroutineName("bridgething-companion"))

    private val stateMutex = Mutex()
    private var capFlags: CompanionCapabilityFlags = capabilities
    private val glues: MutableMap<String, BridgethingGlue> = java.util.concurrent.ConcurrentHashMap()
    @Volatile private var providerPriority: List<String> = emptyList()
    @Volatile private var lastPlayedFromGlueId: String? = null
    private var dispatchers: MutableList<Job> = mutableListOf()
    private var started: Boolean = false
    private var nowPlayingObserver: ((GlueNowPlaying?) -> Unit)? = null
    private var ancsAuthStateObserver: ((AncsAuthState) -> Unit)? = null
    private var logObserver: ((CompanionLogLevel, String) -> Unit)? = null
    private val deviceLogMutex = Mutex()
    private var deviceLogStreaming: Boolean = false
    private var localLogStreaming: Boolean = false
    private val connectedDeviceIds: MutableSet<String> = mutableSetOf()

    private val deviceAutoResume: MutableMap<String, Boolean> = mutableMapOf()
    private val lastAutoResumeAtMs: MutableMap<String, Long> = mutableMapOf()
    internal var autoResumeCooldownMs: Long = AUTO_RESUME_COOLDOWN_MS

    private val deviceLogTokens: MutableMap<String, String> = mutableMapOf()
    private var deviceLogJob: Job? = null
    private var volumeClaimed: Boolean = false
    private var timeChangeReceiver: BroadcastReceiver? = null

    public suspend fun start() {
        stateMutex.withLock {
            if (started) return
            gateway.start()
            spawnDispatchers()
            started = true
        }
        log(CompanionLogLevel.Info, "companion started")
        val receiver = object : BroadcastReceiver() {
            override fun onReceive(context: Context?, intent: Intent?) {
                scope.launch { emitTimeSnapshot() }
            }
        }
        val filter = IntentFilter().apply {
            addAction(Intent.ACTION_TIMEZONE_CHANGED)
            addAction(Intent.ACTION_TIME_CHANGED)
        }
        runCatching { context.registerReceiver(receiver, filter) }
        timeChangeReceiver = receiver
        volumeMonitor.start { level, muted ->
            scope.launch {
                broadcastVolume(level, muted)
                if (!volumeClaimed) {
                    runCatching { gateway.authority.claim(AuthorityClaim(scope = CompanionAuthorityScope.Volume, appBundle = null)) }
                    volumeClaimed = true
                }
            }
        }
    }

    public suspend fun stop() {
        val toCancel: List<Job>
        val attached: List<BridgethingGlue>
        stateMutex.withLock {
            toCancel = dispatchers.toList()
            dispatchers.clear()
            attached = glues.values.toList()
            glues.clear()
            started = false
        }
        for (job in toCancel) job.cancel()
        systemMediaSource.stop()
        nowPlayingHub.stop()

        deviceLogMutex.withLock {
            deviceLogJob?.cancel()
            deviceLogJob = null
            deviceLogTokens.clear()
            connectedDeviceIds.clear()
            deviceLogStreaming = false
        }
        localLogStreaming = false
        refreshLocalLogSink()

        timeChangeReceiver?.let { runCatching { context.unregisterReceiver(it) } }
        timeChangeReceiver = null

        volumeMonitor.stop()
        if (volumeClaimed) {
            runCatching { gateway.authority.release(AuthorityRelease(scope = CompanionAuthorityScope.Volume)) }
            volumeClaimed = false
        }

        runCatching { geoController.stop() }
        runCatching { netDispatcher.stop() }
        runCatching { tunnelDispatcher.stop() }
        runCatching { audioDispatcher.stop() }
        runCatching { phoneDispatcher.stop() }
        runCatching { notificationDispatcher.stop() }
        runCatching { ota.stop() }

        for (g in attached) {
            runCatching { g.detach() }
            runCatching { g.setNowPlayingSink(null) }
        }

        gateway.stop()
        log(CompanionLogLevel.Info, "companion stopped")
    }

    public suspend fun attach(glue: BridgethingGlue) {
        if (stateMutex.withLock { glues.containsKey(glue.name) }) detach(glue.name)
        nowPlayingObserver?.let { glue.setNowPlayingObserver(it) }
        glue.setNowPlayingSink(nowPlayingHub)
        try {
            glue.attach(gateway)
            stateMutex.withLock { glues[glue.name] = glue }
            nowPlayingHub.register(glue.name, glue)
            refreshActiveAppBundles()
            log(CompanionLogLevel.Info, "attached glue ${glue.name}")
        } catch (e: Throwable) {
            runCatching { glue.setNowPlayingSink(null) }
            log(CompanionLogLevel.Error, "glue ${glue.name} attach failed: ${e.message ?: e.toString()}")
            throw e
        }
        announceCapabilities()
    }

    public suspend fun detach(id: String) {
        val glue = stateMutex.withLock { glues.remove(id) } ?: return
        log(CompanionLogLevel.Info, "detaching glue $id")
        nowPlayingHub.unregister(id)
        nowPlayingHub.clearSource(id)
        runCatching { glue.detach() }
        runCatching { glue.setNowPlayingSink(null) }
        stateMutex.withLock { if (lastPlayedFromGlueId == id) lastPlayedFromGlueId = null }
        refreshActiveAppBundles()
        if (stateMutex.withLock { glues.isEmpty() }) nowPlayingObserver?.invoke(null)
        announceCapabilities()
    }

    public suspend fun detachAll() {
        for (id in stateMutex.withLock { glues.keys.toList() }) detach(id)
    }

    public fun attachedProviderIds(): List<String> = glues.keys.toList()

    public suspend fun setProviderPriority(ids: List<String>) {
        stateMutex.withLock { providerPriority = ids }
        announceCapabilities()
    }

    public fun libraryGlue(): BridgethingGlue? {
        lastPlayedFromGlueId?.let { id -> glues[id]?.let { return it } }
        for (id in providerPriority) glues[id]?.let { return it }
        return glues.values.firstOrNull()
    }

    public fun audibleGlue(): BridgethingGlue? = nowPlayingHub.currentSource()?.let { glues[it] }

    private fun orderedGlueIds(): List<String> {
        val ranked = providerPriority.filter { glues.containsKey(it) }
        return ranked + glues.keys.filter { it !in ranked }.sorted()
    }

    private fun attachedSchemes(): List<String> =
        orderedGlueIds().mapNotNull { glues[it] }.flatMap { it.uriSchemes }.distinct()

    private fun glueForUri(uri: String): BridgethingGlue? {
        val scheme = uri.substringBefore(':', "").lowercase()
        if (scheme.isEmpty()) return null
        return orderedGlueIds().mapNotNull { glues[it] }
            .firstOrNull { g -> g.uriSchemes.any { it.lowercase() == scheme } }
    }

    private fun refreshActiveAppBundles() {
        activeAppBundles = glues.values.flatMap { it.appBundles }.toSet()
    }

    private suspend fun notifyPeerConnected(deviceId: String) {
        val allowResume = allowAutoResume(deviceId)
        val winner = resumeWinnerId()
        for ((id, glue) in glues) {
            runCatching { glue.handlePeerConnected(allowResume && id == winner) }
        }
    }

    private fun resumeWinnerId(): String? {
        nowPlayingHub.currentSource()?.let { if (glues.containsKey(it)) return it }
        lastPlayedFromGlueId?.let { if (glues.containsKey(it)) return it }
        return orderedGlueIds().firstOrNull()
    }

    private suspend fun resolveAsset(id: String): AssetBytes? {
        val owner = id.substringBefore('/', "")
        glues[owner]?.asset(id)?.let { return it }
        for ((glueId, glue) in glues) {
            if (glueId == owner) continue
            glue.asset(id)?.let { return it }
        }
        return null
    }

    public fun refreshSystemMedia() {
        systemMediaSource.refresh()
    }

    public suspend fun setCapabilityFlags(flags: CompanionCapabilityFlags) {
        stateMutex.withLock { capFlags = flags }
        notificationsEnabled = flags.notifications
        announceCapabilities()
    }

    public suspend fun setDeviceAutoResume(deviceId: String, enabled: Boolean) {
        deviceLogMutex.withLock { deviceAutoResume[deviceId] = enabled }
    }

    private suspend fun allowAutoResume(deviceId: String): Boolean = deviceLogMutex.withLock {
        if (!(deviceAutoResume[deviceId] ?: true)) {
            log(CompanionLogLevel.Info, "auto-resume off for $deviceId; skipping connect resume")
            return@withLock false
        }
        val resumed = lastAutoResumeAtMs[deviceId]
        if (resumed != null) {
            val sinceMs = System.currentTimeMillis() - resumed
            if (sinceMs < autoResumeCooldownMs) {
                log(CompanionLogLevel.Info, "auto-resumed ${sinceMs / 1000}s ago for $deviceId; skipping connect resume")
                return@withLock false
            }
        }
        lastAutoResumeAtMs[deviceId] = System.currentTimeMillis()
        true
    }

    public suspend fun setNowPlayingObserver(observer: ((GlueNowPlaying?) -> Unit)?) {
        val attached = stateMutex.withLock {
            nowPlayingObserver = observer
            glues.values.toList()
        }
        for (g in attached) g.setNowPlayingObserver(observer ?: { _ -> })
    }

    public fun setAncsAuthStateObserver(observer: ((AncsAuthState) -> Unit)?) {
        ancsAuthStateObserver = observer
    }

    public fun setLogObserver(observer: ((CompanionLogLevel, String) -> Unit)?) {
        logObserver = observer
        refreshLocalLogSink()
    }

    public fun setLocalLogStreaming(enabled: Boolean) {
        if (enabled == localLogStreaming) return
        localLogStreaming = enabled
        refreshLocalLogSink()
    }

    private fun refreshLocalLogSink() {
        val observer = logObserver
        if (!localLogStreaming || observer == null) {
            LocalLogRelay.setSink(null)
            return
        }
        LocalLogRelay.setSink { level, target, message ->
            val companionLevel = when (level) {
                "ERROR" -> CompanionLogLevel.Error
                "WARN" -> CompanionLogLevel.Warn
                "INFO" -> CompanionLogLevel.Info
                else -> CompanionLogLevel.Debug
            }
            val line = "[$target] $message"
            DeviceLogRing.push(companionLevel.raw, line)
            observer(companionLevel, line)
        }
    }

    public suspend fun setDeviceLogStreaming(enabled: Boolean) {
        val toSubscribe = deviceLogMutex.withLock {
            if (enabled == deviceLogStreaming) return
            deviceLogStreaming = enabled
            if (enabled) {
                connectedDeviceIds.toList()
            } else {
                deviceLogJob?.cancel()
                deviceLogJob = null
                emptyList()
            }
        }
        if (enabled) {
            startDeviceLogConsumer()
            for (id in toSubscribe) subscribeDeviceLogs(id)
        } else {
            val tokens = deviceLogMutex.withLock {
                val t = deviceLogTokens.values.toList()
                deviceLogTokens.clear()
                t
            }
            for (token in tokens) runCatching { gateway.system.logsUnsubscribe(LogsUnsubscribe(token = token)) }
        }
    }

    private suspend fun startDeviceLogConsumer() {
        deviceLogMutex.withLock {
            if (deviceLogJob != null) return
            deviceLogJob = scope.launch {
                gateway.system.logEntry.collect { (_, entry) -> forwardDeviceLog(entry) }
            }
        }
    }

    private suspend fun subscribeDeviceLogs(deviceId: String) {
        val result = runCatching {
            gateway.system.logsSubscribe(
                deviceId,
                LogsSubscribe(source = LogSource.Daemon, levels = emptyList(), filter = null),
            )
        }.getOrNull()
        if (result is RequestResult.Ok) deviceLogMutex.withLock { deviceLogTokens[deviceId] = result.response.token }
    }

    private fun forwardDeviceLog(entry: LogEntry) {
        val level = when (entry.level) {
            LogLevel.Trace, LogLevel.Debug -> CompanionLogLevel.Debug
            LogLevel.Info -> CompanionLogLevel.Info
            LogLevel.Warn -> CompanionLogLevel.Warn
            LogLevel.Error -> CompanionLogLevel.Error
        }
        val message = "[${entry.target}] ${entry.message}"
        LogStore.write("daemon ${level.raw}: $message")
        DeviceLogRing.push(level.raw, message)
        logObserver?.invoke(level, message)
    }

    public fun enableAncsNotifications(): AncsSetupResult =
        AncsSetupResult(kind = AncsSetupKind.Unsupported, authState = AncsAuthState.Unknown)

    public fun currentAncsAuthState(): AncsAuthState = AncsAuthState.Unknown

    private suspend fun announceCapabilities() {
        val caps = composeCapabilities()
        runCatching { gateway.capabilities.announce(caps) }
    }

    private fun composeCapabilities(): GatewayCapabilities {
        val glue = libraryGlue()
        val info = GatewayInfo(
            address = host.address,
            name = host.appName,
            osName = host.osName,
            appName = host.appName,
            appVersion = host.appVersion,
            adapterVersion = host.adapterVersion,
            libVersion = BridgethingCompanionVersion.LIB,
            libbridgethingVersion = BridgethingCompanionVersion.LIBBRIDGETHING,
        )
        val avail = SurfaceAvailability(
            geo = capFlags.geo,
            notifications = capFlags.notifications,
            netFetch = capFlags.netFetch,
            netWs = capFlags.netWs,
            audioTts = capFlags.audioTts,
            lyrics = true,
            playbackTargets = glues.values.any { it.supportsPlaybackTargets },
        )
        return GatewayCapabilities(
            gateway = info,
            uriSchemes = attachedSchemes(),
            network = NetworkInfo(kind = NetworkKind.Unknown, metered = false),
            available = avail,
            audio = AudioCapabilities(earcons = emptyList(), voices = emptyList()),
            musicProvider = glue?.musicProvider ?: MusicProvider.None,
        )
    }

    private fun spawnDispatchers() {
        nowPlayingHub.start(scope)
        nowPlayingHub.register(SystemMediaSource.SOURCE_ID, systemMediaSource)
        systemMediaSource.start()
        dispatchers.add(scope.launch { runConnectAnnouncer() })
        dispatchers.add(scope.launch { runKeepaliveResponder() })
        dispatchers.add(
            scope.launch {
                gateway.transfer.ack.collect { (_, ack) -> transferAcks.note(ack.transferId, ack.received) }
            },
        )
        transferReceiver.start(scope)
        dispatchers.add(scope.launch { runPlayerDispatch() })
        dispatchers.add(scope.launch { runAssetDispatch() })
        dispatchers.add(scope.launch { runLyricsDispatch() })
        dispatchers.add(scope.launch { runAncsAuthDispatch() })
        dispatchers.add(scope.launch { runWebappProfileDispatch() })
        dispatchers.add(scope.launch { notificationDispatcher.start(gateway) })
        dispatchers.add(scope.launch { runLibraryDispatch() })
        dispatchers.add(scope.launch { netDispatcher.start(gateway) })
        dispatchers.add(scope.launch { tunnelDispatcher.start(gateway) })
        audioDispatcher.setGlueProvider { audibleGlue() ?: libraryGlue() }
        dispatchers.add(scope.launch { audioDispatcher.start(gateway) })
        dispatchers.add(scope.launch { phoneDispatcher.start(gateway) })
        dispatchers.add(scope.launch { ota.start(gateway) })
        dispatchers.add(scope.launch { geoController.start(gateway) })
    }

    private suspend fun runKeepaliveResponder() {
        gateway.system.keepaliveRequests.collect { (handle, req) ->
            handle.respond(KeepaliveAck(seq = req.seq))
        }
    }

    private suspend fun runWebappProfileDispatch() {
        gateway.webapp.activeChanged.collect { (_, changed) ->
            val hero = changed.art?.heroPx?.toInt() ?: 248
            val thumb = changed.art?.thumbPx?.toInt() ?: 96
            for (g in glues.values) g.setArtProfile(hero, thumb)
        }
    }

    private suspend fun runConnectAnnouncer() {
        gateway.events.collect { event ->
            when (event) {
                is GatewayEvent.Connected -> {
                    log(CompanionLogLevel.Info, "peer connected: ${event.device.name} [${event.device.id}]")
                    announceCapabilities()
                    emitTimeSnapshot()
                    nowPlayingHub.onConnect()
                    notifyPeerConnected(event.device.id)
                    phoneDispatcher.announce(gateway)
                    val subscribe = deviceLogMutex.withLock {
                        connectedDeviceIds.add(event.device.id)
                        deviceLogStreaming
                    }
                    if (subscribe) subscribeDeviceLogs(event.device.id)
                }
                is GatewayEvent.Disconnected -> {
                    log(CompanionLogLevel.Info, "peer disconnected: ${event.deviceId}")
                    deviceLogMutex.withLock {
                        connectedDeviceIds.remove(event.deviceId)
                        deviceLogTokens.remove(event.deviceId)
                    }
                }
                is GatewayEvent.LinkFailed -> {
                    log(CompanionLogLevel.Warn, "peer link failed: ${event.device.name} [${event.device.id}]: ${event.reason}")
                    deviceLogMutex.withLock {
                        connectedDeviceIds.remove(event.device.id)
                        deviceLogTokens.remove(event.device.id)
                    }
                }
                is GatewayEvent.DecodeError -> log(CompanionLogLevel.Warn, "[${event.deviceId}] decode error: ${event.description}")
                is GatewayEvent.Message -> Unit
            }
        }
    }

    private suspend fun emitTimeSnapshot() {
        runCatching { gateway.time.snapshot(currentTimeInfo()) }
    }

    private fun currentTimeInfo(): TimeInfo {
        val nowMs = System.currentTimeMillis()
        val tz = java.util.TimeZone.getDefault()
        val dstMs = if (tz.inDaylightTime(java.util.Date(nowMs))) tz.dstSavings else 0
        return TimeInfo(
            tzIana = tz.id,
            locale = java.util.Locale.getDefault().toLanguageTag(),
            wallClockUnixS = (nowMs / 1000L).coerceIn(0L, UInt.MAX_VALUE.toLong()).toUInt(),
            utcOffsetMinutes = (tz.getOffset(nowMs) / 60000).toShort(),
            dstOffsetMinutes = (dstMs / 60000).toByte(),
        )
    }

    private suspend fun runPlayerDispatch() {
        gateway.events.collect { event ->
            if (event !is GatewayEvent.Message) return@collect
            val data = event.message.data as? BridgeToGatewayMsgData.Player ?: return@collect
            scope.launch { dispatchPlayer(data.data) }
        }
    }

    private suspend fun dispatchPlayer(player: BridgeToGatewayPlayerMsg) {
        val transport: NowPlayingTransport? = nowPlayingHub.currentTransport() ?: libraryGlue()
        try {
            when (player) {
                is BridgeToGatewayPlayerMsg.Play -> {
                    val glue = glueForUri(player.data.uri)
                    if (glue != null) {
                        lastPlayedFromGlueId = glue.name
                        glue.play(player.data)
                    } else {
                        log(CompanionLogLevel.Warn, "play dropped: no provider claims ${player.data.uri}")
                    }
                }
                is BridgeToGatewayPlayerMsg.Queue -> {
                    val glue = glueForUri(player.data.uri)
                    if (glue != null) glue.queue(player.data)
                    else log(CompanionLogLevel.Warn, "queue dropped: no provider claims ${player.data.uri}")
                }
                BridgeToGatewayPlayerMsg.Pause -> transport?.pause()
                BridgeToGatewayPlayerMsg.Resume -> transport?.resume()
                BridgeToGatewayPlayerMsg.SkipNext -> transport?.skipNext()
                BridgeToGatewayPlayerMsg.SkipPrev -> transport?.skipPrev()
                is BridgeToGatewayPlayerMsg.SkipToIndex -> transport?.skipToIndex(player.data.index)
                is BridgeToGatewayPlayerMsg.SeekTo -> transport?.seekTo(player.data.positionMs)
                is BridgeToGatewayPlayerMsg.SetShuffle -> transport?.setShuffle(player.data.on)
                is BridgeToGatewayPlayerMsg.SetRepeat -> transport?.setRepeat(player.data.mode)
                is BridgeToGatewayPlayerMsg.SetSpeed -> transport?.setSpeed(player.data.speed)
                is BridgeToGatewayPlayerMsg.SetCrossfade -> transport?.setCrossfade(player.data.durationMs)
                is BridgeToGatewayPlayerMsg.TransferTo -> {
                    val glue = audibleGlue() ?: libraryGlue()
                    if (glue != null) glue.transferTo(player.data.targetId)
                    else log(CompanionLogLevel.Warn, "transferTo dropped: no music provider")
                }
            }
        } catch (e: Throwable) {
            log(CompanionLogLevel.Warn, "player verb $player failed: ${e.message ?: e.toString()}")
        }
    }

    private suspend fun runAssetDispatch() {
        gateway.asset.requestRequests.collect { (handle, req) ->
            scope.launch { handleAsset(handle, req) }
        }
    }

    private suspend fun handleAsset(handle: AssetRequestHandle, req: AssetRequest) {
        val bytes: AssetBytes? = try {
            if (req.id.startsWith(SystemMediaSource.ASSET_ID_PREFIX)) systemMediaSource.asset(req.id)
            else resolveAsset(req.id)
        } catch (e: Throwable) {
            log(CompanionLogLevel.Warn, "asset ${req.id} glue resolve failed: ${e.message ?: e.toString()}")
            runCatching { handle.respondErr(AssetNotFoundReply(id = req.id)) }
            return
        }
        if (bytes == null) {
            runCatching { handle.respondErr(AssetNotFoundReply(id = req.id)) }
            return
        }
        runCatching { streamAsset(handle, req.id, req.requestId, bytes) }
            .onFailure { log(CompanionLogLevel.Warn, "asset ${req.id} respond failed: ${it.message ?: it.toString()}") }
    }

    private suspend fun streamAsset(handle: AssetRequestHandle, id: String, requestId: UUID, payload: AssetBytes) {
        val data = payload.bytes
        if (data.size <= INLINE_BODY_MAX_BYTES) {
            handle.respond(AssetGotReply(id = id, mime = payload.mime, body = TransferBody.Inline(data)))
            return
        }
        handle.respond(
            AssetGotReply(
                id = id,
                mime = payload.mime,
                body = TransferBody.Stream(TransferRef(id = requestId, totalSize = data.size.toUInt(), sha256 = null)),
            ),
            priority = Priority.Normal,
        )
        sendFragments(handle.deviceId, requestId, data, ASSET_FRAGMENT_BYTES, Priority.Background, transferAcks)
    }

    internal suspend fun sendFragments(
        deviceId: String,
        transferId: UUID,
        data: ByteArray,
        fragmentBytes: Int,
        priority: Priority,
        acks: TransferAckWindow? = null,
    ) {
        var offset = 0
        while (offset < data.size) {
            if (acks != null) {
                while (true) {
                    val acked = acks.receivedBytes(transferId)
                    if (offset < acked.toInt() + TRANSFER_WINDOW_BYTES) break
                    if (!acks.waitForProgress(transferId, acked, TRANSFER_ACK_TIMEOUT_MS)) {
                        acks.finish(transferId)
                        runCatching {
                            gateway.device(deviceId).transfer.abandon(
                                TransferAbandon(transferId = transferId, reason = "ack timeout"),
                            )
                        }
                        error("transfer stalled: fragment acks stopped")
                    }
                }
            }
            val end = minOf(offset + fragmentBytes, data.size)
            gateway.device(deviceId).transfer.fragment(
                TransferFragment(transferId = transferId, offset = offset.toUInt(), bytes = data.copyOfRange(offset, end)),
                priority = priority,
            )
            offset = end
        }
        acks?.finish(transferId)
    }

    private suspend fun runLyricsDispatch() {
        gateway.lyrics.getRequests.collect { (handle, req) ->
            scope.launch { handleLyrics(handle, req) }
        }
    }

    private suspend fun handleLyrics(handle: LyricsRequestHandle, req: LyricsRequest) {
        val identity = TrackIdentity(
            artist = req.track.artist,
            track = req.track.track,
            album = req.track.album,
            durationMs = req.track.durationMs?.toInt(),
            isrc = req.track.isrc,
        )
        val resolved: DomainLyrics? = try {
            (audibleGlue() ?: libraryGlue())?.lyrics(identity) ?: lyricsResolver.lyrics(identity)
        } catch (e: Throwable) {
            log(CompanionLogLevel.Warn, "lyrics resolve failed for ${req.track.artist} - ${req.track.track}: ${e.message ?: e.toString()}")
            runCatching { handle.respondErr(LyricsErrorReply(message = e.toString())) }
            return
        }
        val wire: WireLyrics? = resolved?.let(::toWireLyrics)
        runCatching { handle.respond(LyricsReply(lyrics = wire)) }
    }

    private suspend fun runAncsAuthDispatch() {
        gateway.notifications.ancsAuthStateChanged.collect { (_, state) ->
            ancsAuthStateObserver?.invoke(state)
        }
    }

    // MARK: - library dispatch

    private suspend fun runLibraryDispatch(): Unit = coroutineScope {
        launch { gateway.library.browseRequests.collect { (handle, req) -> launch { handleBrowse(handle, req) } } }
        launch { gateway.library.resolveContextRequests.collect { (handle, req) -> launch { handleResolveContext(handle, req) } } }
        launch { gateway.library.searchRequests.collect { (handle, req) -> launch { handleSearch(handle, req) } } }
        launch { gateway.library.recommendationsRequests.collect { (handle, req) -> launch { handleRecommendations(handle, req) } } }
        launch { gateway.library.favoritesListRequests.collect { (handle, req) -> launch { handleFavoritesList(handle, req) } } }
        launch { gateway.library.favoritesContainsRequests.collect { (handle, req) -> launch { handleFavoritesContains(handle, req) } } }
        launch { gateway.library.favoritesToggle.collect { (_, msg) -> launch { handleFavoritesToggle(msg) } } }
        launch { gateway.library.favoritesSet.collect { (_, msg) -> launch { handleFavoritesSet(msg) } } }
        launch { gateway.library.favoritesSetMany.collect { (_, msg) -> launch { handleFavoritesSetMany(msg) } } }
    }

    private suspend fun handleBrowse(handle: LibraryBrowseRequestHandle, req: LibraryBrowseRequest) {
        val glue = libraryGlue() ?: run { runCatching { handle.respondErr(noProviderReply()) }; return }
        val result = try {
            glue.browse(req)
        } catch (e: Throwable) {
            respondLibraryError(e, { runCatching { handle.respondProtocolErr(it) } }, { runCatching { handle.respondErr(it) } })
            return
        }
        val isRoot = req.nodeId == null || req.nodeId == "" || req.nodeId == "root"
        runCatching {
            if (isRoot) {
                handle.respond(BrowseReply(result), priority = Priority.Bulk, compression = Compression.GZIP)
            } else {
                handle.respond(BrowseReply(result))
            }
        }
    }

    private suspend fun handleResolveContext(handle: LibraryResolveContextRequestHandle, req: LibraryResolveContextRequest) {
        val glue = libraryGlue() ?: run { runCatching { handle.respondErr(noProviderReply()) }; return }
        val result = try {
            glue.resolveContext(req.uri)
        } catch (e: Throwable) {
            respondLibraryError(e, { runCatching { handle.respondProtocolErr(it) } }, { runCatching { handle.respondErr(it) } })
            return
        }
        runCatching { handle.respond(result) }
    }

    private suspend fun handleSearch(handle: LibrarySearchRequestHandle, req: LibrarySearchRequest) {
        val glue = libraryGlue() ?: run { runCatching { handle.respondErr(noProviderReply()) }; return }
        val result = try {
            glue.search(req)
        } catch (e: Throwable) {
            respondLibraryError(e, { runCatching { handle.respondProtocolErr(it) } }, { runCatching { handle.respondErr(it) } })
            return
        }
        runCatching { handle.respond(SearchReply(result)) }
    }

    private suspend fun handleRecommendations(handle: LibraryRecommendationsRequestHandle, req: LibraryRecommendationsRequest) {
        val glue = libraryGlue() ?: run { runCatching { handle.respondErr(noProviderReply()) }; return }
        val result = try {
            glue.recommendations(req)
        } catch (e: Throwable) {
            respondLibraryError(e, { runCatching { handle.respondProtocolErr(it) } }, { runCatching { handle.respondErr(it) } })
            return
        }
        runCatching { handle.respond(RecommendationsReply(result)) }
    }

    private suspend fun handleFavoritesList(handle: LibraryFavoritesListRequestHandle, req: LibraryFavoritesListRequest) {
        val glue = libraryGlue() ?: run { runCatching { handle.respondErr(noProviderReply()) }; return }
        val page = try {
            glue.favoritesList(req)
        } catch (e: Throwable) {
            respondLibraryError(e, { runCatching { handle.respondProtocolErr(it) } }, { runCatching { handle.respondErr(it) } })
            return
        }
        runCatching { handle.respond(FavoritesListReply(page)) }
    }

    private suspend fun handleFavoritesContains(handle: LibraryFavoritesContainsRequestHandle, req: LibraryFavoritesContainsRequest) {
        val glue = libraryGlue() ?: run { runCatching { handle.respondErr(noProviderReply()) }; return }
        val liked = try {
            glue.favoritesContains(req)
        } catch (e: Throwable) {
            respondLibraryError(e, { runCatching { handle.respondProtocolErr(it) } }, { runCatching { handle.respondErr(it) } })
            return
        }
        runCatching { handle.respond(FavoritesContainsReply(liked)) }
    }

    private suspend fun handleFavoritesToggle(msg: FavoritesToggle) {
        if (systemMediaSource.owns(msg.item.uri)) {
            systemMediaSource.toggleLiked()
            return
        }
        val glue = glueForUri(msg.item.uri) ?: libraryGlue() ?: return
        try {
            glue.favoritesToggle(msg.item)
        } catch (e: Throwable) {
            log(CompanionLogLevel.Warn, "favoritesToggle failed: ${e.message ?: e.toString()}")
        }
    }

    private suspend fun handleFavoritesSet(msg: FavoritesSet) {
        if (systemMediaSource.owns(msg.item.uri)) {
            systemMediaSource.setLiked(msg.liked)
            return
        }
        val glue = glueForUri(msg.item.uri) ?: libraryGlue() ?: return
        try {
            glue.favoritesSet(msg.item, msg.liked)
        } catch (e: Throwable) {
            log(CompanionLogLevel.Warn, "favoritesSet failed: ${e.message ?: e.toString()}")
        }
    }

    private suspend fun handleFavoritesSetMany(msg: FavoritesSetMany) {
        val byProvider = msg.entries.groupBy { (glueForUri(it.item.uri) ?: libraryGlue())?.name }
        for ((id, entries) in byProvider) {
            val glue = id?.let { glues[it] } ?: continue
            try {
                glue.favoritesSetMany(entries)
            } catch (e: Throwable) {
                log(CompanionLogLevel.Warn, "favoritesSetMany failed for $id: ${e.message ?: e.toString()}")
            }
        }
    }

    private fun noProviderReply(): LibraryErrorReply =
        LibraryErrorReply(LibraryError.NotSupported(LibraryErrorNotSupportedInner(reason = "no active music provider")))

    private suspend fun respondLibraryError(
        error: Throwable,
        onProtocol: suspend (WireError) -> Unit,
        onDomain: suspend (LibraryErrorReply) -> Unit,
    ) {
        when (error) {
            is GlueError.NotImplemented -> onProtocol(WireError.Unimplemented)
            is GlueError.NotAuthenticated -> onDomain(LibraryErrorReply(LibraryError.Unauthorized))
            is GlueError.Detached ->
                onDomain(LibraryErrorReply(LibraryError.NotSupported(LibraryErrorNotSupportedInner(reason = "music provider detached"))))
            is GlueError.Underlying ->
                onDomain(LibraryErrorReply(LibraryError.NotSupported(LibraryErrorNotSupportedInner(reason = error.cause?.toString() ?: error.toString()))))
            else ->
                onDomain(LibraryErrorReply(LibraryError.NotSupported(LibraryErrorNotSupportedInner(reason = error.toString()))))
        }
    }

    private suspend fun broadcastVolume(level: Float, muted: Boolean) {
        runCatching {
            gateway.audio.volumeChanged(VolumeChanged(level = level, muted = muted))
        }
    }

    private fun log(level: CompanionLogLevel, message: String) {
        when (level) {
            CompanionLogLevel.Debug -> android.util.Log.d(TAG, message)
            CompanionLogLevel.Info -> android.util.Log.i(TAG, message)
            CompanionLogLevel.Warn -> android.util.Log.w(TAG, message)
            CompanionLogLevel.Error -> android.util.Log.e(TAG, message)
        }
        DeviceLogRing.push(level.raw, message)
        logObserver?.invoke(level, message)
    }

    private fun toWireLyrics(lyrics: DomainLyrics): WireLyrics = WireLyrics(
        synced = lyrics.synced?.map { line ->
            LyricLine(
                startMs = line.startMs.coerceAtLeast(0).toUInt(),
                text = line.text,
            )
        },
        plain = lyrics.plain,
        source = lyrics.source,
    )

    private companion object {
        const val AUTO_RESUME_COOLDOWN_MS = 5L * 60L * 1000L
        const val TAG = "bridgething.companion"
        const val ASSET_FRAGMENT_BYTES = 4 * 1024
        const val INLINE_BODY_MAX_BYTES = 8 * 1024
        const val TRANSFER_WINDOW_BYTES = 64 * 1024
        const val TRANSFER_ACK_TIMEOUT_MS = 15_000L
    }
}
