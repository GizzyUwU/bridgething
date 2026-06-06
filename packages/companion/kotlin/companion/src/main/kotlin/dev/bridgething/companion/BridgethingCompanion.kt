package dev.bridgething.companion

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import dev.bridgething.gateway.Adapter
import dev.bridgething.gateway.AssetRequestHandle
import dev.bridgething.gateway.BridgethingGateway
import dev.bridgething.gateway.GatewayEvent
import dev.bridgething.gateway.LyricsRequestHandle
import dev.bridgething.gateway.RequestResult
import dev.bridgething.gateway.asset
import dev.bridgething.gateway.audio
import dev.bridgething.gateway.authority
import dev.bridgething.gateway.capabilities
import dev.bridgething.gateway.device
import dev.bridgething.gateway.lyrics
import dev.bridgething.gateway.notifications
import dev.bridgething.gateway.system
import dev.bridgething.gateway.time
import dev.bridgething.gateway.transfer
import dev.bridgething.gateway.webapp
import dev.bridgething.glue.AssetBytes
import dev.bridgething.glue.BridgethingGlue
import dev.bridgething.glue.GlueNowPlaying
import dev.bridgething.lyrics.Lyrics as DomainLyrics
import dev.bridgething.lyrics.LyricsResolver
import dev.bridgething.lyrics.TrackIdentity
import dev.bridgething.schema.AncsAuthState
import dev.bridgething.schema.AssetGotReply
import dev.bridgething.schema.Priority
import java.util.UUID
import dev.bridgething.schema.AssetNotFoundReply
import dev.bridgething.schema.AssetRequest
import dev.bridgething.schema.AudioCapabilities
import dev.bridgething.schema.AuthorityClaim
import dev.bridgething.schema.AuthorityRelease
import dev.bridgething.schema.BridgeToGatewayMsgData
import dev.bridgething.schema.BridgeToGatewayPlayerMsg
import dev.bridgething.schema.CompanionAuthorityScope
import dev.bridgething.schema.GatewayCapabilities
import dev.bridgething.schema.GatewayInfo
import dev.bridgething.schema.LogEntry
import dev.bridgething.schema.LogLevel
import dev.bridgething.schema.LogSource
import dev.bridgething.schema.LogsSubscribe
import dev.bridgething.schema.LogsUnsubscribe
import dev.bridgething.schema.LyricLine
import dev.bridgething.schema.Lyrics as WireLyrics
import dev.bridgething.schema.LyricsErrorReply
import dev.bridgething.schema.LyricsReply
import dev.bridgething.schema.LyricsRequest
import dev.bridgething.schema.MusicProvider
import dev.bridgething.schema.NetworkInfo
import dev.bridgething.schema.NetworkKind
import dev.bridgething.schema.SurfaceAvailability
import dev.bridgething.schema.TimeInfo
import dev.bridgething.schema.TransferBody
import dev.bridgething.schema.TransferFragment
import dev.bridgething.schema.TransferRef
import dev.bridgething.schema.VolumeChanged
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
import dev.bridgething.gateway.library
import dev.bridgething.gateway.LibraryBrowseRequestHandle
import dev.bridgething.gateway.LibraryResolveContextRequestHandle
import dev.bridgething.gateway.LibrarySearchRequestHandle
import dev.bridgething.gateway.LibraryRecommendationsRequestHandle
import dev.bridgething.gateway.LibraryFavoritesListRequestHandle
import dev.bridgething.gateway.LibraryFavoritesContainsRequestHandle
import dev.bridgething.glue.GlueError
import dev.bridgething.schema.BrowseReply
import dev.bridgething.schema.SearchReply
import dev.bridgething.schema.RecommendationsReply
import dev.bridgething.schema.FavoritesListReply
import dev.bridgething.schema.FavoritesContainsReply
import dev.bridgething.schema.LibraryErrorReply
import dev.bridgething.schema.LibraryError
import dev.bridgething.schema.LibraryErrorNotSupportedInner
import dev.bridgething.schema.WireError
import dev.bridgething.schema.LibraryBrowseRequest
import dev.bridgething.schema.LibraryResolveContextRequest
import dev.bridgething.schema.LibrarySearchRequest
import dev.bridgething.schema.LibraryRecommendationsRequest
import dev.bridgething.schema.LibraryFavoritesListRequest
import dev.bridgething.schema.LibraryFavoritesContainsRequest
import dev.bridgething.schema.FavoritesToggle
import dev.bridgething.schema.FavoritesSet
import dev.bridgething.schema.FavoritesSetMany

/** Severity tag passed to the [BridgethingCompanion] log observer. */
public enum class CompanionLogLevel(public val raw: String) {
    Debug("debug"), Info("info"), Warn("warn"), Error("error"),
}

/** version stamps the companion announces in `GatewayInfo`. */
public object BridgethingCompanionVersion {
    public const val LIB: String = "0.1.0"
    public const val LIBBRIDGETHING: String = "0.1.0"
}

/**
 * Identity the companion advertises in `GatewayCapabilities.gateway`.
 * Caller-supplied at companion init; on Android the natural value for
 * `address` is `Settings.Secure.ANDROID_ID`. Empty string is acceptable
 * when no stable identifier is available.
 */
public data class HostInfo(
    val appName: String,
    val appVersion: String,
    val osName: String,
    val osVersion: String = "",
    val address: String = "",
    val adapterVersion: String = "",
)

/**
 * Capability flags the companion declares. Glue contributions
 * (`uriSchemes`, `musicProvider`, `lyricsSupported`) are mixed in by
 * [BridgethingCompanion] at announce time.
 */
public data class CompanionCapabilityFlags(
    val geo: Boolean = true,
    val notifications: Boolean = false,
    val netFetch: Boolean = true,
    val netWs: Boolean = true,
    val audioTts: Boolean = true,
)

/** outcome of an attempted ANCS pair flow; Android has no AccessorySetupKit equivalent so always [Unsupported] here. */
public enum class AncsSetupKind {
    Paired, AlreadyPaired, Cancelled, Unsupported, Failed
}

public data class AncsSetupResult(
    val kind: AncsSetupKind,
    val authState: AncsAuthState,
    val message: String? = null,
)

/**
 * Top-level orchestrator for the bridgething companion on Android. Owns one [BridgethingGateway] over the
 * supplied adapter and holds at most one active [BridgethingGlue]; per-state mutation flows through [stateMutex].
 */
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
    notificationActions: NotificationActionBackend = NoOpNotificationActionBackend,
    phone: PhoneBackend = NoOpPhoneBackend,
) {
    public val gateway: BridgethingGateway = BridgethingGateway(adapter)
    public val ota: OtaService = OtaService(httpClient = httpClient)
    public val catalog: CatalogService = CatalogService(
        installer = ota,
        store = FileCatalogStore(
            java.io.File(context.filesDir?.path ?: (System.getProperty("java.io.tmpdir") ?: "."), "bridgething-catalog"),
        ),
        httpClient = httpClient,
    )

    private val netDispatcher = NetDispatcher(client = httpClient)
    private val tunnelDispatcher = TunnelDispatcher()
    private val audioDispatcher = AudioDispatcher(backend = audio)
    private val phoneDispatcher = PhoneDispatcher(backend = phone)
    private val notificationActions: NotificationActionBackend = notificationActions
    private val geoController: GeoSource = geo
    private val volumeMonitor: VolumeSource = volume

    private val supervisor: CompletableJob = SupervisorJob()
    private val scope = CoroutineScope(supervisor + Dispatchers.Default + CoroutineName("bridgething-companion"))

    private val stateMutex = Mutex()
    private var capFlags: CompanionCapabilityFlags = capabilities
    private var activeGlue: BridgethingGlue? = null
    private var dispatchers: MutableList<Job> = mutableListOf()
    private var started: Boolean = false
    private var nowPlayingObserver: ((GlueNowPlaying?) -> Unit)? = null
    private var ancsAuthStateObserver: ((AncsAuthState) -> Unit)? = null
    private var logObserver: ((CompanionLogLevel, String) -> Unit)? = null
    private val deviceLogMutex = Mutex()
    private var deviceLogStreaming: Boolean = false
    private val connectedDeviceIds: MutableSet<String> = mutableSetOf()
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
        // the device has no battery RTC; re-seed its clock on tz/wall-clock changes.
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
        val glue: BridgethingGlue?
        stateMutex.withLock {
            toCancel = dispatchers.toList()
            dispatchers.clear()
            glue = activeGlue
            activeGlue = null
            started = false
        }
        for (job in toCancel) job.cancel()

        deviceLogMutex.withLock {
            deviceLogJob?.cancel()
            deviceLogJob = null
            deviceLogTokens.clear()
            connectedDeviceIds.clear()
            deviceLogStreaming = false
        }

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
        runCatching { ota.stop() }
        runCatching { catalog.stop() }

        if (glue != null) runCatching { glue.detach() }

        gateway.stop()
        log(CompanionLogLevel.Info, "companion stopped")
    }

    public suspend fun setActive(glue: BridgethingGlue?) {
        val previous = stateMutex.withLock {
            val prev = activeGlue
            activeGlue = glue
            prev
        }
        if (previous != null) {
            log(CompanionLogLevel.Info, "detaching glue ${previous.name}")
            runCatching { previous.detach() }
            nowPlayingObserver?.invoke(null)
        }
        if (glue != null) {
            nowPlayingObserver?.let { glue.setNowPlayingObserver(it) }
            try {
                glue.attach(gateway)
                log(CompanionLogLevel.Info, "attached glue ${glue.name}")
            } catch (e: Throwable) {
                log(CompanionLogLevel.Error, "glue ${glue.name} attach failed: ${e.message ?: e.toString()}")
                throw e
            }
        }
        announceCapabilities()
    }

    public fun current(): BridgethingGlue? = activeGlue

    public suspend fun setCapabilityFlags(flags: CompanionCapabilityFlags) {
        stateMutex.withLock { capFlags = flags }
        announceCapabilities()
    }

    /** observer persists across [setActive] swaps and takes effect immediately for the current glue. */
    public suspend fun setNowPlayingObserver(observer: ((GlueNowPlaying?) -> Unit)?) {
        val glue = stateMutex.withLock {
            nowPlayingObserver = observer
            activeGlue
        }
        glue?.setNowPlayingObserver(observer ?: { _ -> })
    }

    /** iOS-only signal in practice; on Android the observer never fires from a daemon-side ANCS event. */
    public fun setAncsAuthStateObserver(observer: ((AncsAuthState) -> Unit)?) {
        ancsAuthStateObserver = observer
    }

    public fun setLogObserver(observer: ((CompanionLogLevel, String) -> Unit)?) {
        logObserver = observer
    }

    // subscribes to the daemon's tracing log over the gateway and forwards entries through logObserver.
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
        DeviceLogRing.push(level.raw, message)
        logObserver?.invoke(level, message)
    }

    /** Android has no equivalent to the iOS ANCS pair flow; always resolves [AncsSetupKind.Unsupported]. */
    public fun enableAncsNotifications(): AncsSetupResult =
        AncsSetupResult(kind = AncsSetupKind.Unsupported, authState = AncsAuthState.Unknown)

    public fun currentAncsAuthState(): AncsAuthState = AncsAuthState.Unknown

    private suspend fun announceCapabilities() {
        val caps = composeCapabilities()
        runCatching { gateway.capabilities.announce(caps) }
    }

    private fun composeCapabilities(): GatewayCapabilities {
        val glue = activeGlue
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
        )
        return GatewayCapabilities(
            gateway = info,
            uriSchemes = glue?.uriSchemes ?: emptyList(),
            network = NetworkInfo(kind = NetworkKind.Unknown, metered = false),
            available = avail,
            audio = AudioCapabilities(earcons = emptyList(), voices = emptyList()),
            musicProvider = glue?.musicProvider ?: MusicProvider.None,
        )
    }

    private fun spawnDispatchers() {
        dispatchers.add(scope.launch { runConnectAnnouncer() })
        dispatchers.add(scope.launch { runPlayerDispatch() })
        dispatchers.add(scope.launch { runAssetDispatch() })
        dispatchers.add(scope.launch { runLyricsDispatch() })
        dispatchers.add(scope.launch { runAncsAuthDispatch() })
        dispatchers.add(scope.launch { runWebappProfileDispatch() })
        dispatchers.add(scope.launch { runNotificationInvokeDispatch() })
        dispatchers.add(scope.launch { runLibraryDispatch() })
        dispatchers.add(scope.launch { netDispatcher.start(gateway) })
        dispatchers.add(scope.launch { tunnelDispatcher.start(gateway) })
        dispatchers.add(scope.launch { audioDispatcher.start(gateway) })
        dispatchers.add(scope.launch { phoneDispatcher.start(gateway) })
        dispatchers.add(scope.launch { ota.start(gateway) })
        dispatchers.add(scope.launch { catalog.start(gateway) })
        dispatchers.add(scope.launch { geoController.start(gateway) })
    }

    private suspend fun runWebappProfileDispatch() {
        gateway.webapp.activeChanged.collect { (_, changed) ->
            val hero = changed.art?.heroPx?.toInt() ?: 248
            val thumb = changed.art?.thumbPx?.toInt() ?: 96
            activeGlue?.setArtProfile(hero, thumb)
        }
    }

    private suspend fun runConnectAnnouncer() {
        gateway.events.collect { event ->
            when (event) {
                is GatewayEvent.Connected -> {
                    log(CompanionLogLevel.Info, "peer connected: ${event.device.name} [${event.device.id}]")
                    announceCapabilities()
                    emitTimeSnapshot()
                    activeGlue?.handlePeerConnected()
                    phoneDispatcher.announce(gateway)
                    val subscribe = deviceLogMutex.withLock {
                        connectedDeviceIds.add(event.device.id)
                        deviceLogStreaming
                    }
                    if (subscribe) subscribeDeviceLogs(event.device.id)
                }
                is GatewayEvent.Disconnected -> {
                    log(CompanionLogLevel.Info, "peer disconnected: ${event.deviceId}")
                    // the daemon auto-releases the subscription token on peer disconnect.
                    deviceLogMutex.withLock {
                        connectedDeviceIds.remove(event.deviceId)
                        deviceLogTokens.remove(event.deviceId)
                    }
                }
                is GatewayEvent.DecodeError -> log(CompanionLogLevel.Warn, "[${event.deviceId}] decode error: ${event.description}")
                is GatewayEvent.Message -> Unit
            }
        }
    }

    // the device has no battery-backed RTC; the companion is the wall-clock authority.
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
            val glue = activeGlue ?: return@collect
            scope.launch { dispatchPlayer(data.data, glue) }
        }
    }

    private suspend fun dispatchPlayer(player: BridgeToGatewayPlayerMsg, glue: BridgethingGlue) {
        try {
            when (player) {
                is BridgeToGatewayPlayerMsg.Play -> glue.play(player.data)
                is BridgeToGatewayPlayerMsg.Queue -> glue.queue(player.data)
                BridgeToGatewayPlayerMsg.Pause -> glue.pause()
                BridgeToGatewayPlayerMsg.Resume -> glue.resume()
                BridgeToGatewayPlayerMsg.SkipNext -> glue.skipNext()
                BridgeToGatewayPlayerMsg.SkipPrev -> glue.skipPrev()
                is BridgeToGatewayPlayerMsg.SkipToIndex -> glue.skipToIndex(player.data.index)
                is BridgeToGatewayPlayerMsg.SeekTo -> glue.seekTo(player.data.positionMs)
                is BridgeToGatewayPlayerMsg.SetShuffle -> glue.setShuffle(player.data.on)
                is BridgeToGatewayPlayerMsg.SetRepeat -> glue.setRepeat(player.data.mode)
                is BridgeToGatewayPlayerMsg.SetSpeed -> glue.setSpeed(player.data.speed)
                is BridgeToGatewayPlayerMsg.SetCrossfade -> glue.setCrossfade(player.data.durationMs)
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
            activeGlue?.asset(req.id)
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
        )
        sendFragments(handle.deviceId, requestId, data, ASSET_FRAGMENT_BYTES, Priority.Bulk)
    }

    internal suspend fun sendFragments(
        deviceId: String,
        transferId: UUID,
        data: ByteArray,
        fragmentBytes: Int,
        priority: Priority,
    ) {
        var offset = 0
        while (offset < data.size) {
            val end = minOf(offset + fragmentBytes, data.size)
            gateway.device(deviceId).transfer.fragment(
                TransferFragment(transferId = transferId, offset = offset.toUInt(), bytes = data.copyOfRange(offset, end)),
                priority = priority,
            )
            offset = end
        }
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
            activeGlue?.lyrics(identity) ?: lyricsResolver.lyrics(identity)
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

    // iOS routes notification actions over ANCS, so this path is Android-only.
    private suspend fun runNotificationInvokeDispatch(): Unit = coroutineScope {
        launch { gateway.notifications.invokePositive.collect { (_, msg) -> launch { notificationActions.invokePositive(msg.id) } } }
        launch { gateway.notifications.invokeNegative.collect { (_, msg) -> launch { notificationActions.invokeNegative(msg.id) } } }
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
        val glue = activeGlue ?: run { runCatching { handle.respondErr(noProviderReply()) }; return }
        val result = try {
            glue.browse(req)
        } catch (e: Throwable) {
            respondLibraryError(e, { runCatching { handle.respondProtocolErr(it) } }, { runCatching { handle.respondErr(it) } })
            return
        }
        runCatching { handle.respond(BrowseReply(result)) }
    }

    private suspend fun handleResolveContext(handle: LibraryResolveContextRequestHandle, req: LibraryResolveContextRequest) {
        val glue = activeGlue ?: run { runCatching { handle.respondErr(noProviderReply()) }; return }
        val result = try {
            glue.resolveContext(req.uri)
        } catch (e: Throwable) {
            respondLibraryError(e, { runCatching { handle.respondProtocolErr(it) } }, { runCatching { handle.respondErr(it) } })
            return
        }
        runCatching { handle.respond(result) }
    }

    private suspend fun handleSearch(handle: LibrarySearchRequestHandle, req: LibrarySearchRequest) {
        val glue = activeGlue ?: run { runCatching { handle.respondErr(noProviderReply()) }; return }
        val result = try {
            glue.search(req)
        } catch (e: Throwable) {
            respondLibraryError(e, { runCatching { handle.respondProtocolErr(it) } }, { runCatching { handle.respondErr(it) } })
            return
        }
        runCatching { handle.respond(SearchReply(result)) }
    }

    private suspend fun handleRecommendations(handle: LibraryRecommendationsRequestHandle, req: LibraryRecommendationsRequest) {
        val glue = activeGlue ?: run { runCatching { handle.respondErr(noProviderReply()) }; return }
        val result = try {
            glue.recommendations(req)
        } catch (e: Throwable) {
            respondLibraryError(e, { runCatching { handle.respondProtocolErr(it) } }, { runCatching { handle.respondErr(it) } })
            return
        }
        runCatching { handle.respond(RecommendationsReply(result)) }
    }

    private suspend fun handleFavoritesList(handle: LibraryFavoritesListRequestHandle, req: LibraryFavoritesListRequest) {
        val glue = activeGlue ?: run { runCatching { handle.respondErr(noProviderReply()) }; return }
        val page = try {
            glue.favoritesList(req)
        } catch (e: Throwable) {
            respondLibraryError(e, { runCatching { handle.respondProtocolErr(it) } }, { runCatching { handle.respondErr(it) } })
            return
        }
        runCatching { handle.respond(FavoritesListReply(page)) }
    }

    private suspend fun handleFavoritesContains(handle: LibraryFavoritesContainsRequestHandle, req: LibraryFavoritesContainsRequest) {
        val glue = activeGlue ?: run { runCatching { handle.respondErr(noProviderReply()) }; return }
        val liked = try {
            glue.favoritesContains(req)
        } catch (e: Throwable) {
            respondLibraryError(e, { runCatching { handle.respondProtocolErr(it) } }, { runCatching { handle.respondErr(it) } })
            return
        }
        runCatching { handle.respond(FavoritesContainsReply(liked)) }
    }

    private suspend fun handleFavoritesToggle(msg: FavoritesToggle) {
        val glue = activeGlue ?: return
        try {
            glue.favoritesToggle(msg.item)
        } catch (e: Throwable) {
            log(CompanionLogLevel.Warn, "favoritesToggle failed: ${e.message ?: e.toString()}")
        }
    }

    private suspend fun handleFavoritesSet(msg: FavoritesSet) {
        val glue = activeGlue ?: return
        try {
            glue.favoritesSet(msg.item, msg.liked)
        } catch (e: Throwable) {
            log(CompanionLogLevel.Warn, "favoritesSet failed: ${e.message ?: e.toString()}")
        }
    }

    private suspend fun handleFavoritesSetMany(msg: FavoritesSetMany) {
        val glue = activeGlue ?: return
        try {
            glue.favoritesSetMany(msg.entries)
        } catch (e: Throwable) {
            log(CompanionLogLevel.Warn, "favoritesSetMany failed: ${e.message ?: e.toString()}")
        }
    }

    private fun noProviderReply(): LibraryErrorReply =
        LibraryErrorReply(LibraryError.NotSupported(LibraryErrorNotSupportedInner(reason = "no active music provider")))

    // notImplemented maps to protocol Unimplemented (recognized verb, no backend); all other errors become domain replies.
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
        const val TAG = "bridgething.companion"
        const val ASSET_FRAGMENT_BYTES = 8 * 1024
        const val INLINE_BODY_MAX_BYTES = 8 * 1024
    }
}
