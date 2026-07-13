package com.bridgething.spotify

import com.bridgething.gateway.BridgethingGateway
import com.bridgething.gateway.library
import com.bridgething.glue.AssetBytes
import com.bridgething.glue.BridgethingGlue
import com.bridgething.glue.GlueAuthState
import com.bridgething.glue.GlueCapability
import com.bridgething.glue.GlueDebugState
import com.bridgething.glue.GlueDeviceCodePrompt
import com.bridgething.glue.GlueError
import com.bridgething.glue.GlueNowPlaying
import com.bridgething.glue.GlueServiceHealth
import com.bridgething.glue.NowPlayingSink
import com.bridgething.schema.BrowseEntry
import com.bridgething.schema.BrowseFolder
import com.bridgething.schema.BrowseResult
import com.bridgething.schema.ContextResolveReply
import com.bridgething.schema.FavoritesPage
import com.bridgething.schema.FavoritesSet
import com.bridgething.schema.ItemKind
import com.bridgething.schema.ItemRef
import com.bridgething.schema.LibraryBrowseRequest
import com.bridgething.schema.LibraryChanged
import com.bridgething.schema.LibraryFavoritesContainsRequest
import com.bridgething.schema.LibraryFavoritesListRequest
import com.bridgething.schema.LibraryItem
import com.bridgething.schema.LibraryRecommendationsRequest
import com.bridgething.schema.LibrarySearchRequest
import com.bridgething.schema.MediaItem
import com.bridgething.schema.MediaItemUpdate
import com.bridgething.schema.MusicProvider
import com.bridgething.schema.NowPlayingUpdate
import com.bridgething.schema.Playback
import com.bridgething.schema.PlaybackContext
import com.bridgething.schema.PlaybackState
import com.bridgething.schema.PlaybackUpdate
import com.bridgething.schema.PlayUri
import com.bridgething.schema.PlayerOptions
import com.bridgething.schema.QueueItem
import com.bridgething.schema.QueuePosition
import com.bridgething.schema.QueueSnapshot
import com.bridgething.schema.QueueUri
import com.bridgething.schema.RecommendationsResult
import com.bridgething.schema.SearchResult
import com.bridgething.schema.ShuffleMode
import com.bridgething.schema.Station
import io.ktor.client.HttpClient
import io.ktor.client.engine.cio.CIO
import io.ktor.client.plugins.HttpTimeout
import io.ktor.client.request.get
import io.ktor.client.statement.HttpResponse
import io.ktor.client.statement.bodyAsBytes
import java.lang.ref.WeakReference
import java.net.URLDecoder
import java.net.URLEncoder
import java.util.concurrent.locks.ReentrantLock
import kotlin.concurrent.withLock
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancelChildren
import kotlinx.coroutines.launch
import com.bridgething.schema.Album as WireAlbum
import com.bridgething.schema.Artist as WireArtist
import com.bridgething.schema.LibraryScope as WireLibraryScope
import com.bridgething.schema.Playlist as WirePlaylist
import com.bridgething.schema.PlayerState as WirePlayerState
import com.bridgething.schema.PodcastEpisode as WirePodcastEpisode
import com.bridgething.schema.RepeatMode as WireRepeat
import com.bridgething.schema.Show as WireShow
import com.bridgething.schema.Track as WireTrack
import uniffi.spotify.AuthState as SpAuthState
import uniffi.spotify.BrowseItem as SpBrowseItem
import uniffi.spotify.DeviceWaker
import uniffi.spotify.LibraryScope as SpLibraryScope
import uniffi.spotify.Observer as SpObserver
import uniffi.spotify.PlayerState as SpPlayerState
import uniffi.spotify.Queue as SpQueue
import uniffi.spotify.RepeatMode as SpRepeat
import uniffi.spotify.Shelf as SpShelf
import uniffi.spotify.SpotifyClient
import uniffi.spotify.SpotifyClientInterface
import uniffi.spotify.initLogging
import uniffi.spotify.Track as SpTrack
import uniffi.spotify.Device as SpDevice
import uniffi.spotify.TokenStore as SpTokenStore

private const val ASSET_ID_PREFIX = "spotify/img/"
private const val SCDN_IMAGE_PREFIX = "https://i.scdn.co/image/"
private const val BUILTIN_REF_PREFIX = "builtin:"
private const val BUILTIN_ASSET_ID_PREFIX = "builtin/img/"
private const val DEFAULT_HERO_EDGE = 248
private const val DEFAULT_THUMB_EDGE = 96
private const val QUEUE_MAX = 50
private const val QUEUE_RUNWAY_FLOOR = 8
private const val SPOTIFY_APP_BUNDLE = "com.spotify.client"
private const val SPOTIFY_ANDROID_PACKAGE = "com.spotify.music"
private const val VOLUME_STEP_PERCENT = 6.25

typealias SpotifyClientFactory = (store: SpTokenStore, observer: SpObserver) -> SpotifyClientInterface

class SpotifyGlue(
    private val workerBase: String,
    private val psk: String,
    private val deviceId: String,
    private val tokenStore: SpTokenStore,
    cacheDir: java.io.File? = null,
    appContext: android.content.Context? = null,
    private val connectivity: ConnectivityWatcher = NoOpConnectivityWatcher,
    private val clientFactory: SpotifyClientFactory = { store, observer ->
        initLogging(LogcatLogSink(), if (BuildConfig.DEBUG) "spotify=trace" else "spotify=info")
        SpotifyClient.create(workerBase, psk, deviceId, store, observer).also {
            it.setWsTransport(KtorWsTransport())
            it.setHttpTransport(KtorHttpTransport())
        }
    },
) : BridgethingGlue {
    override val name: String = "spotify"
    override val displayName: String = "Spotify"

    override val capabilities: Set<GlueCapability> = setOf(
        GlueCapability.STREAMING,
        GlueCapability.QUEUE,
        GlueCapability.ALBUM_ART,
        GlueCapability.RECOMMENDATIONS,
        GlueCapability.RECENTLY_PLAYED,
        GlueCapability.LIBRARY,
        GlueCapability.PLAYLISTS,
    )

    override val uriSchemes: List<String> = listOf("spotify")
    override val musicProvider: MusicProvider = MusicProvider.Spotify
    override val lyricsSupported: Boolean = false
    override val appBundles: List<String> = listOf(SPOTIFY_ANDROID_PACKAGE)

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val httpClient get() = sharedHttpClient
    private val imageCache: ImageDiskCache? =
        cacheDir?.let { ImageDiskCache(java.io.File(it, "spotify-art"), 200L shl 20) }

    private var client: SpotifyClientInterface? = null
    private var gateway: BridgethingGateway? = null
    private var connectJob: Job? = null
    private var nowPlayingObserver: ((GlueNowPlaying?) -> Unit)? = null
    private var authObserver: ((GlueAuthState) -> Unit)? = null
    private var serviceHealthObserver: ((GlueServiceHealth) -> Unit)? = null

    private val stateLock = ReentrantLock()
    private val likedOverride = mutableMapOf<String, Boolean>()

    @Volatile private var heroEdge = DEFAULT_HERO_EDGE
    @Volatile private var thumbEdge = DEFAULT_THUMB_EDGE
    @Volatile private var lastState: SpPlayerState? = null
    @Volatile private var lastStateAtMs: Long? = null
    @Volatile private var lastQueueItems: List<QueueItem> = emptyList()
    @Volatile private var lastSentQueueOrder: List<String> = emptyList()
    @Volatile private var lastSentThumbEdge = DEFAULT_THUMB_EDGE
    @Volatile private var lastHadItem = false
    @Volatile private var lastKnownDeviceCount: Int? = null
    @Volatile private var wakeOnEmptyCluster = false
    @Volatile private var lastConnectivityAvailable: Boolean? = null

    @Volatile private var sink: NowPlayingSink? = null
    private val localWaker: DeviceWaker? = appContext?.let { IntentDeviceWaker(it) }

    // MARK: - lifecycle

    override suspend fun attach(gateway: BridgethingGateway) {
        if (this.gateway != null) detach()
        this.gateway = gateway
        resetQueueDedup()

        val client = clientFactory(tokenStore, ObserverBridge(this))
        localWaker?.let { client.setDeviceWaker(it) }
        this.client = client

        authObserver?.invoke(GlueAuthState.Pending(null))
        connectJob = scope.launch {
            runCatching { client.connect() }.onFailure {
                authObserver?.invoke(GlueAuthState.Failed("sign-in error: ${it.message}"))
            }
        }

        lastConnectivityAvailable = null
        connectivity.start { available ->
            val restored = lastConnectivityAvailable == false && available
            lastConnectivityAvailable = available
            if (restored) scope.launch { runCatching { client.resync() } }
        }
    }

    override suspend fun detach() {
        authObserver = null
        serviceHealthObserver = null
        connectivity.stop()
        lastConnectivityAvailable = null
        connectJob?.cancel()
        connectJob = null
        runCatching { client?.disconnect() }
        scope.coroutineContext.cancelChildren()
        sink?.clearSource(name)
        nowPlayingObserver?.invoke(null)
        nowPlayingObserver = null
        resetQueueDedup()
        stateLock.withLock { likedOverride.clear() }
        lastState = null
        lastStateAtMs = null
        lastQueueItems = emptyList()
        client = null
        gateway = null
    }

    override suspend fun setNowPlayingObserver(observer: (GlueNowPlaying?) -> Unit) { nowPlayingObserver = observer }
    override suspend fun setNowPlayingSink(sink: NowPlayingSink?) { this.sink = sink }
    override suspend fun setAuthObserver(observer: (GlueAuthState) -> Unit) { authObserver = observer }
    override suspend fun setServiceHealthObserver(observer: (GlueServiceHealth) -> Unit) {
        serviceHealthObserver = observer
        observer(GlueServiceHealth.Ok)
    }

    override suspend fun setArtProfile(heroPx: Int, thumbPx: Int) {
        heroEdge = heroPx.coerceAtLeast(1)
        thumbEdge = thumbPx.coerceAtLeast(1)
    }

    override suspend fun debugState(): GlueDebugState =
        GlueDebugState(authorityPlaybackHeld = lastHadItem, authorityMetadataHeld = lastHadItem)

    override suspend fun handlePeerConnected(allowAutoResume: Boolean) {
        if (gateway == null) return
        if (allowAutoResume) {
            localWaker?.wakeDevice()
            val clusterEmpty = lastKnownDeviceCount?.let { it == 0 } ?: run {
                wakeOnEmptyCluster = true
                null
            }
            if (clusterEmpty == true) spawnConnectResume()
        }
        resetQueueDedup()
        val pending = lastState?.takeIf { it.track != null }
        if (pending != null) {
            val fresh = client?.currentPositionMs()
            val state = if (fresh != null) pending.copy(positionMs = fresh) else pending
            val ageMs = if (fresh != null) null else cachedPositionAgeMs()
            sink?.submitPlayer(name, makeSnapshot(state, positionAgeMs = ageMs), SPOTIFY_APP_BUNDLE, hasItem = true)
        }
        val queue = lastQueueItems
        if (queue.isNotEmpty()) sendQueueChangedIfNeeded(queue, thumbEdge)
    }

    // MARK: - dealer firehose

    private fun onPlayer(state: SpPlayerState) {
        if (gateway == null) return
        val (liked, likeSupported) = likeFields(state.track)
        val update = makeUpdate(state, heroEdge, liked, likeSupported)
        nowPlayingObserver?.invoke(GlueNowPlaying(update = update, artworkUrl = state.track?.let { rawArtworkUrl(bestHex(it)) }))
        lastState = state
        lastStateAtMs = monotonicNowMs()
        val hasItem = state.track != null
        lastHadItem = hasItem
        sink?.submitPlayer(name, makeSnapshot(state, heroEdge, liked, likeSupported), SPOTIFY_APP_BUNDLE, hasItem)
    }

    private fun onDevices(devices: List<SpDevice>) {
        lastKnownDeviceCount = devices.size
        if (wakeOnEmptyCluster) {
            wakeOnEmptyCluster = false
            if (devices.isEmpty()) spawnConnectResume()
        }
    }

    private fun spawnConnectResume() {
        scope.launch { runCatching { client?.resume() } }
    }

    private fun onQueue(queue: SpQueue) {
        val thumb = thumbEdge
        val entries = queue.next.take(QUEUE_MAX).map { queueItem(it, thumb) }
        lastQueueItems = entries
        sendQueueChangedIfNeeded(entries, thumb)
    }

    private fun onLibraryChanged(libraryScope: SpLibraryScope) {
        val gateway = gateway ?: return
        val wireScope = when (libraryScope) {
            SpLibraryScope.SAVED -> WireLibraryScope.Saved
            SpLibraryScope.PLAYLISTS -> WireLibraryScope.Playlists
        }
        scope.launch { runCatching { gateway.library.libraryChanged(LibraryChanged(wireScope)) } }
    }

    private fun onAuth(state: SpAuthState) {
        when (state) {
            is SpAuthState.LoggedIn -> {
                authObserver?.invoke(GlueAuthState.Authenticated)
                scope.launch { checkPremium() }
            }
            is SpAuthState.LoggedOut -> {
                handleAuthDown()
                authObserver?.invoke(GlueAuthState.Pending(null))
            }
            is SpAuthState.Pending -> authObserver?.invoke(GlueAuthState.Pending(GlueDeviceCodePrompt(state.code, state.url, state.url)))
            is SpAuthState.Failed -> {
                handleAuthDown()
                authObserver?.invoke(GlueAuthState.Failed(state.reason))
            }
        }
    }

    private fun handleAuthDown() {
        nowPlayingObserver?.invoke(null)
        lastHadItem = false
        sink?.clearSource(name)
    }

    private suspend fun checkPremium() {
        val product = runCatching { client?.product() }.getOrNull() ?: return
        if (!product.canUseSuperbird) authObserver?.invoke(GlueAuthState.Failed("Spotify Premium is required"))
    }

    // MARK: - inbound transport

    override suspend fun play(uri: PlayUri) {
        val client = client ?: throw GlueError.Detached
        val context = uri.context
        if (context != null) client.play(context.contextUri, uri.uri) else client.play(uri.uri, null)
    }

    override suspend fun queue(req: QueueUri) {
        val client = client ?: throw GlueError.Detached
        if (req.position is QueuePosition.Index) throw GlueError.NotImplemented
        client.queueUri(req.uri)
    }

    override suspend fun pause() { require().pause() }
    override suspend fun resume() { require().resume() }
    override suspend fun skipNext() { require().skipNext() }
    override suspend fun skipPrev() { require().skipPrev() }

    override suspend fun skipToIndex(index: UInt) {
        val client = require()
        val target = lastQueueItems.getOrNull(index.toInt()) ?: return
        val context = lastState?.contextUri?.takeIf { it.isNotEmpty() } ?: return
        client.play(context, target.uri)
    }
    override suspend fun seekTo(positionMs: UInt) { require().seek(positionMs.toLong()) }
    override suspend fun setShuffle(on: Boolean) { require().setShuffle(on) }
    override suspend fun setRepeat(mode: WireRepeat) {
        val mapped = when (mode) {
            WireRepeat.Off -> SpRepeat.OFF
            WireRepeat.All -> SpRepeat.CONTEXT
            WireRepeat.One -> SpRepeat.TRACK
        }
        require().setRepeat(mapped)
    }

    override suspend fun ownsVolume(): Boolean =
        lastState?.let { it.onRemoteSpeaker && it.track != null } == true

    override suspend fun volumeUp(): Float = volumeStepped(VOLUME_STEP_PERCENT)
    override suspend fun volumeDown(): Float = volumeStepped(-VOLUME_STEP_PERCENT)
    override suspend fun setVolume(level: Float): Float {
        require().setVolume(level.toDouble() * 100.0)
        return level
    }

    private suspend fun volumeStepped(deltaPercent: Double): Float =
        (require().volumeStep(deltaPercent) / 100.0).toFloat()

    private fun require(): SpotifyClientInterface = client ?: throw GlueError.Detached

    // MARK: - library

    override suspend fun search(req: LibrarySearchRequest): SearchResult {
        val client = require()
        val kinds = req.kinds?.takeIf { it.isNotEmpty() }
            ?: listOf(ItemKind.Track, ItemKind.Album, ItemKind.Artist, ItemKind.Playlist)
        val res = client.search(req.query, req.limit)
        val edge = heroEdge
        val limit = req.limit.toInt()
        val items = mutableListOf<LibraryItem>()
        val present = mutableListOf<ItemKind>()
        var full = false
        for (kind in kinds) {
            val arr = when (kind) {
                ItemKind.Track -> res.tracks
                ItemKind.Album -> res.albums
                ItemKind.Artist -> res.artists
                ItemKind.Playlist -> res.playlists
                else -> emptyList()
            }
            val mapped = arr.mapNotNull { libraryItem(it, edge) }
            if (mapped.isNotEmpty()) {
                present.add(kind)
                if (mapped.size >= limit) full = true
            }
            items.addAll(mapped)
        }
        return SearchResult(items = items, kinds = present, total = null, hasMore = full)
    }

    override suspend fun browse(req: LibraryBrowseRequest): BrowseResult {
        val client = require()
        val edge = heroEdge
        val result = when (req.nodeId) {
            null, "", "root" -> {
                val shelves = client.rootBrowse(req.sections, req.preview)
                BrowseResult(
                    entries = shelves.map { BrowseEntry.Folder(folder(it, edge)) },
                    total = shelves.size.toUInt(), hasMore = false,
                )
            }
            else -> {
                val page = client.browse(req.nodeId!!, req.limit, req.offset)
                BrowseResult(
                    entries = page.items.mapNotNull { libraryItem(it, edge)?.let { li -> BrowseEntry.Item(li) } },
                    total = page.total, hasMore = page.hasMore,
                )
            }
        }
        warmArt(result)
        return result
    }

    override suspend fun resolveContext(uri: String): ContextResolveReply {
        val b = require().resolveContext(uri)
        return ContextResolveReply(
            name = b.title.ifEmpty { null },
            artworkId = artAssetId(b.imageId, heroEdge),
            subtitle = b.subtitle.ifEmpty { null },
        )
    }

    override suspend fun recommendations(req: LibraryRecommendationsRequest): RecommendationsResult {
        val client = require()
        val edge = heroEdge
        val artist = req.seeds.firstOrNull { it.kind == ItemKind.Artist }
        if (artist != null) {
            val page = client.browse(artist.uri, req.limit, 0u)
            return RecommendationsResult(items = page.items.mapNotNull { libraryItem(it, edge) }, total = null, hasMore = false)
        }
        return RecommendationsResult(items = emptyList(), total = null, hasMore = false)
    }

    override suspend fun favoritesList(req: LibraryFavoritesListRequest): FavoritesPage {
        val client = require()
        val edge = heroEdge
        val page = client.favoritesList(req.limit, req.offset)
        return FavoritesPage(items = page.items.mapNotNull { libraryItem(it, edge) }, total = page.total, hasMore = page.hasMore)
    }

    override suspend fun favoritesContains(req: LibraryFavoritesContainsRequest): List<Boolean> =
        require().favoritesContains(req.uris)

    override suspend fun favoritesToggle(item: ItemRef) {
        val client = require()
        val saved = client.favoritesContains(listOf(item.uri)).firstOrNull() ?: false
        client.favoritesSet(item.uri, !saved)
        applyLikedChange(item.uri, !saved)
    }

    override suspend fun favoritesSet(item: ItemRef, liked: Boolean) {
        require().favoritesSet(item.uri, liked)
        applyLikedChange(item.uri, liked)
    }

    override suspend fun favoritesSetMany(entries: List<FavoritesSet>) {
        val client = require()
        for (entry in entries) {
            client.favoritesSet(entry.item.uri, entry.liked)
            applyLikedChange(entry.item.uri, entry.liked)
        }
    }

    // MARK: - assets

    override suspend fun asset(id: String): AssetBytes? {
        val (urlString, maxEdge) = parseImageId(id) ?: return null
        val master = fetchMaster(urlString) ?: return null
        val scaled = downsample(master, maxEdge) ?: return null
        return AssetBytes(bytes = scaled, mime = "image/jpeg")
    }

    private suspend fun fetchMaster(url: String): ByteArray? {
        imageCache?.get(url)?.let { return it }
        val response: HttpResponse = httpClient.get(url)
        if (response.status.value !in 200..299) return null
        val data = response.bodyAsBytes()
        imageCache?.put(url, data)
        return data
    }

    private fun warmArt(result: BrowseResult) {
        for (id in collectArtIds(result.entries).toSet()) {
            val url = parseImageId(id)?.first ?: continue
            scope.launch { fetchMaster(url) }
        }
    }

    private fun collectArtIds(entries: List<BrowseEntry>): List<String> =
        entries.flatMap { entry ->
            when (entry) {
                is BrowseEntry.Folder ->
                    listOfNotNull(entry.data.artworkId) + collectArtIds(entry.data.previewChildren ?: emptyList())
                is BrowseEntry.Item -> listOfNotNull(libraryItemArtworkId(entry.data))
            }
        }

    private fun libraryItemArtworkId(item: LibraryItem): String? = when (item) {
        is LibraryItem.Track -> item.data.image_id.ifEmpty { null }
        is LibraryItem.Playlist -> item.data.artworkId
        is LibraryItem.PodcastEpisode -> item.data.artworkId
        is LibraryItem.Show -> item.data.artworkId
        is LibraryItem.Station -> item.data.artworkId
        is LibraryItem.Album -> item.data.artwork_id
        is LibraryItem.Artist -> item.data.artwork_id
    }

    // MARK: - outbound snapshot / queue

    private fun monotonicNowMs(): Long = System.nanoTime() / 1_000_000

    // how stale the cached lastState position already is; stamped onto re-sends of cached
    // snapshots so the daemon re-anchors them onto live time instead of reading a rewind
    private fun cachedPositionAgeMs(): UInt? =
        lastStateAtMs?.let { (monotonicNowMs() - it).coerceAtLeast(0L).toUInt() }

    private fun makeSnapshot(state: SpPlayerState, positionAgeMs: UInt? = null): WirePlayerState {
        val (liked, supported) = likeFields(state.track)
        return makeSnapshot(state, heroEdge, liked, supported, positionAgeMs)
    }

    private fun makeSnapshot(
        state: SpPlayerState,
        heroEdge: Int,
        liked: Boolean?,
        likeSupported: Boolean?,
        positionAgeMs: UInt? = null,
    ): WirePlayerState {
        val track: MediaItem? = state.track?.let { t ->
            MediaItem(
                uri = t.uri,
                persistentId = t.uri,
                title = t.name.ifEmpty { null },
                album = t.album.name.ifEmpty { null },
                albumUri = t.album.uri.ifEmpty { null },
                albumArtist = null,
                artist = artistNames(t),
                artistUri = t.artists.firstOrNull()?.uri,
                liked = liked,
                artworkId = artAssetId(bestHex(t), heroEdge),
                durationMs = t.durationMs,
                mediaTypes = null,
                trackNumber = null,
                trackCount = null,
                isLikeSupported = likeSupported,
                isBanSupported = null,
                isBanned = null,
                chapterCount = null,
            )
        }
        val playback = Playback(
            state = if (state.isPaused) PlaybackState.Paused else PlaybackState.Playing,
            positionMs = state.positionMs,
            positionAgeMs = positionAgeMs,
            shuffle = state.shuffle,
            shuffleMode = if (state.shuffle) ShuffleMode.Songs else ShuffleMode.Off,
            repeat = mapRepeat(state.repeat),
            queueIndex = null,
            queueCount = null,
            queueChapterIndex = null,
            setElapsedTimeAvailable = state.canSeek,
            queueListAvail = null,
            appleMusicRadioAd = null,
        )
        val context = if (state.contextUri.isEmpty()) null else
            PlaybackContext(uri = state.contextUri, name = state.contextName.ifEmpty { null })
        return WirePlayerState(
            track = track,
            playback = playback,
            queue = emptyList(),
            options = PlayerOptions(speed = 1.0f, crossfade_ms = null),
            context = context,
        )
    }

    private fun sendQueueChangedIfNeeded(entries: List<QueueItem>, thumb: Int) {
        val sink = sink ?: return
        val order = entries.map { it.uri }
        val edgeChanged = thumb != lastSentThumbEdge
        lastSentThumbEdge = thumb
        if (!edgeChanged) {
            val runway = forwardSlideRunway(lastSentQueueOrder, order)
            if (runway != null && runway >= QUEUE_RUNWAY_FLOOR) return
        }
        sink.submitQueue(name, QueueSnapshot(order = order, items = entries))
        lastSentQueueOrder = order
    }

    private fun resetQueueDedup() {
        lastSentQueueOrder = emptyList()
    }

    private fun forwardSlideRunway(last: List<String>, new: List<String>): Int? {
        if (last.isEmpty()) return null
        for (k in 1 until last.size) {
            val suffix = last.subList(k, last.size)
            if (new.size >= suffix.size && new.subList(0, suffix.size) == suffix) return suffix.size
        }
        return null
    }

    // MARK: - liked

    /**
     * the liked flag for a track: rust-provided saved (cluster-first, warm-cache fallback)
     * unless a pending user toggle overrides it. the override drops once rust catches up.
     */
    private fun likeFields(track: SpTrack?): Pair<Boolean?, Boolean?> {
        if (track == null || !isSpotifyUri(track.uri)) return null to null
        val liked = stateLock.withLock {
            val override = likedOverride[track.uri] ?: return@withLock track.saved
            if (override == track.saved) likedOverride.remove(track.uri)
            override
        }
        return liked to true
    }

    private suspend fun applyLikedChange(uri: String, liked: Boolean) {
        stateLock.withLock { likedOverride[uri] = liked }
        reemitSnapshotIfCurrent(uri)
    }

    private fun reemitSnapshotIfCurrent(uri: String) {
        if (gateway == null) return
        val pending = lastState ?: return
        if (pending.track?.uri != uri) return
        sink?.submitPlayer(
            name,
            makeSnapshot(pending, positionAgeMs = cachedPositionAgeMs()),
            SPOTIFY_APP_BUNDLE,
            hasItem = pending.track != null,
        )
    }

    /** adapts the FFI Observer callbacks to the glue (weak, so the rust handle never pins it). */
    private class ObserverBridge(glue: SpotifyGlue) : SpObserver {
        private val glue = WeakReference(glue)
        override fun onPlayer(state: SpPlayerState) { glue.get()?.onPlayer(state) }
        override fun onQueue(queue: SpQueue) { glue.get()?.onQueue(queue) }
        override fun onDevices(devices: List<SpDevice>) { glue.get()?.onDevices(devices) }
        override fun onAuth(state: SpAuthState) { glue.get()?.onAuth(state) }
        override fun onLibraryChanged(scope: SpLibraryScope) { glue.get()?.onLibraryChanged(scope) }
    }

    private companion object {
        val sharedHttpClient: HttpClient by lazy {
            HttpClient(CIO) {
                install(HttpTimeout) {
                    requestTimeoutMillis = 6_000
                    connectTimeoutMillis = 4_000
                }
            }
        }

        fun artistNames(t: SpTrack): String? = t.artists.joinToString(", ") { it.name }.ifEmpty { null }

        fun bestHex(t: SpTrack): String = t.imageId.ifEmpty { t.album.imageId }

        fun artAssetId(ref: String, edge: Int): String? =
            if (ref.isEmpty()) null
            else if (ref.startsWith(BUILTIN_REF_PREFIX)) "$BUILTIN_ASSET_ID_PREFIX${ref.removePrefix(BUILTIN_REF_PREFIX)}"
            else imageAssetId(if (ref.startsWith("http")) ref else "$SCDN_IMAGE_PREFIX$ref", edge)

        fun rawArtworkUrl(ref: String): String? =
            if (ref.isEmpty()) null else if (ref.startsWith("http")) ref else "$SCDN_IMAGE_PREFIX$ref"

        fun isSpotifyUri(uri: String): Boolean = uri.startsWith("spotify:")

        fun kindOf(uri: String): String = uri.split(":").getOrNull(1) ?: ""

        fun mapRepeat(mode: SpRepeat): WireRepeat = when (mode) {
            SpRepeat.OFF -> WireRepeat.Off
            SpRepeat.CONTEXT -> WireRepeat.All
            SpRepeat.TRACK -> WireRepeat.One
        }

        fun mapTrack(b: SpBrowseItem, edge: Int): WireTrack = WireTrack(
            id = b.uri,
            name = b.title,
            album = WireAlbum(id = b.album.uri, name = b.album.name, artwork_id = null),
            artist = WireArtist(id = b.artists.firstOrNull()?.uri ?: "", name = b.artists.firstOrNull()?.name ?: "", artwork_id = null),
            artists = b.artists.map { WireArtist(id = it.uri, name = it.name, artwork_id = null) },
            duration_ms = b.durationMs,
            image_id = artAssetId(b.imageId, edge) ?: "",
            saved = b.saved,
        )

        fun libraryItem(b: SpBrowseItem, edge: Int): LibraryItem? {
            val art = artAssetId(b.imageId, edge)
            return when (kindOf(b.uri)) {
                "track" -> LibraryItem.Track(mapTrack(b, edge))
                "album" -> LibraryItem.Album(WireAlbum(id = b.uri, name = b.title, artwork_id = art))
                "artist" -> LibraryItem.Artist(WireArtist(id = b.uri, name = b.title, artwork_id = art))
                "playlist" -> LibraryItem.Playlist(WirePlaylist(uri = b.uri, name = b.title, ownerName = null, trackCount = null, artworkId = art))
                "user" -> if (b.uri.endsWith(":collection")) {
                    LibraryItem.Playlist(WirePlaylist(uri = b.uri, name = b.title, ownerName = null, trackCount = null, artworkId = art))
                } else {
                    null
                }
                "show" -> LibraryItem.Show(WireShow(uri = b.uri, name = b.title, publisher = b.subtitle.ifEmpty { null }, episodeCount = null, artworkId = art))
                "episode" -> LibraryItem.PodcastEpisode(WirePodcastEpisode(uri = b.uri, name = b.title, showName = b.subtitle.ifEmpty { null }, durationMs = b.durationMs, publishedAtUnixS = null, artworkId = art))
                "station" -> LibraryItem.Station(Station(uri = b.uri, name = b.title, seed = null, artworkId = art))
                else -> null
            }
        }

        fun folder(s: SpShelf, edge: Int): BrowseFolder {
            val children = s.items.mapNotNull { libraryItem(it, edge)?.let { li -> BrowseEntry.Item(li) } }
            return BrowseFolder(
                nodeId = s.id, title = s.title, subtitle = null, artworkId = null,
                total = s.total, previewChildren = children.ifEmpty { null },
            )
        }

        fun queueItem(t: SpTrack, edge: Int): QueueItem = QueueItem(
            uri = t.uri,
            title = t.name.ifEmpty { null },
            artist = artistNames(t),
            artistUri = t.artists.firstOrNull()?.uri,
            album = t.album.name.ifEmpty { null },
            albumUri = t.album.uri.ifEmpty { null },
            artworkId = artAssetId(bestHex(t), edge),
            durationMs = t.durationMs,
            persistentId = null,
            queued = t.queued,
        )

        fun makeUpdate(state: SpPlayerState, heroEdge: Int, liked: Boolean?, likeSupported: Boolean?): NowPlayingUpdate {
            val media: MediaItemUpdate? = state.track?.let { t ->
                MediaItemUpdate(
                    persistentId = t.uri,
                    title = t.name.ifEmpty { null },
                    album = t.album.name.ifEmpty { null },
                    albumUri = t.album.uri.ifEmpty { null },
                    albumArtist = null,
                    artist = artistNames(t),
                    artistUri = t.artists.firstOrNull()?.uri,
                    liked = liked,
                    artworkId = artAssetId(bestHex(t), heroEdge),
                    durationMs = t.durationMs,
                    mediaTypes = null,
                    trackNumber = null,
                    trackCount = null,
                    isLikeSupported = likeSupported,
                    isBanSupported = null,
                    isBanned = null,
                    isResidentOnDevice = null,
                    chapterCount = null,
                )
            }
            val playback = PlaybackUpdate(
                playing = !state.isPaused,
                positionMs = state.positionMs,
                shuffle = state.shuffle,
                shuffleMode = if (state.shuffle) ShuffleMode.Songs else ShuffleMode.Off,
                repeat = mapRepeat(state.repeat),
                appBundle = SPOTIFY_APP_BUNDLE,
                appDisplayName = "Spotify",
                queueIndex = null,
                queueCount = null,
                queueChapterIndex = null,
                playbackSpeed = null,
                setElapsedTimeAvailable = state.canSeek,
                queueListAvail = null,
                appleMusicRadioAd = null,
                appleMusicRadioStationName = null,
            )
            return NowPlayingUpdate(mediaItem = media, playback = playback)
        }

        fun imageAssetId(rawUrl: String, maxEdge: Int): String? {
            if (rawUrl.isEmpty()) return null
            if (rawUrl.startsWith(SCDN_IMAGE_PREFIX)) {
                return "$ASSET_ID_PREFIX$maxEdge/i${rawUrl.substring(SCDN_IMAGE_PREFIX.length)}"
            }
            val encoded = URLEncoder.encode(rawUrl, "UTF-8").replace("+", "%20").replace("*", "%2A").replace("%7E", "~")
            return "$ASSET_ID_PREFIX$maxEdge/u$encoded"
        }

        fun parseImageId(id: String): Pair<String, Int>? {
            if (!id.startsWith(ASSET_ID_PREFIX)) return null
            val rest = id.substring(ASSET_ID_PREFIX.length)
            val slash = rest.indexOf('/')
            if (slash <= 0) return null
            val maxEdge = rest.substring(0, slash).toIntOrNull() ?: return null
            val tagged = rest.substring(slash + 1)
            if (tagged.isEmpty()) return null
            val body = tagged.substring(1)
            val url = when (tagged[0]) {
                'i' -> SCDN_IMAGE_PREFIX + body
                'u' -> runCatching { URLDecoder.decode(body, "UTF-8") }.getOrNull() ?: return null
                else -> return null
            }
            return url to maxEdge
        }

        fun downsample(data: ByteArray, maxEdge: Int): ByteArray? = runCatching {
            val bounds = android.graphics.BitmapFactory.Options().apply { inJustDecodeBounds = true }
            android.graphics.BitmapFactory.decodeByteArray(data, 0, data.size, bounds)
            val longest = maxOf(bounds.outWidth, bounds.outHeight)
            if (longest <= 0) return@runCatching null
            var sample = 1
            while (longest / (sample * 2) >= maxEdge) sample *= 2
            val opts = android.graphics.BitmapFactory.Options().apply { inSampleSize = sample }
            val bmp = android.graphics.BitmapFactory.decodeByteArray(data, 0, data.size, opts) ?: return@runCatching null
            val scale = maxEdge.toFloat() / maxOf(bmp.width, bmp.height).toFloat()
            val scaled = if (scale < 1f) {
                android.graphics.Bitmap.createScaledBitmap(
                    bmp, (bmp.width * scale).toInt().coerceAtLeast(1), (bmp.height * scale).toInt().coerceAtLeast(1), true,
                )
            } else {
                bmp
            }
            val out = java.io.ByteArrayOutputStream()
            scaled.compress(android.graphics.Bitmap.CompressFormat.JPEG, 60, out)
            out.toByteArray()
        }.getOrNull()
    }
}

private class IntentDeviceWaker(context: android.content.Context) : DeviceWaker {
    private val appContext = context.applicationContext

    override fun wakeDevice() {
        for (action in intArrayOf(android.view.KeyEvent.ACTION_DOWN, android.view.KeyEvent.ACTION_UP)) {
            val intent = android.content.Intent(android.content.Intent.ACTION_MEDIA_BUTTON).apply {
                setPackage(SPOTIFY_ANDROID_PACKAGE)
                putExtra(
                    android.content.Intent.EXTRA_KEY_EVENT,
                    android.view.KeyEvent(action, android.view.KeyEvent.KEYCODE_MEDIA_PLAY),
                )
            }
            runCatching { appContext.sendBroadcast(intent) }
        }
    }
}
