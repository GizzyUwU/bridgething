package dev.bridgething.spotify

import dev.bridgething.gateway.BridgethingGateway
import dev.bridgething.gateway.DiagnosticsBuffer
import dev.bridgething.gateway.authority
import dev.bridgething.gateway.player
import dev.bridgething.glue.AssetBytes
import dev.bridgething.glue.BridgethingGlue
import dev.bridgething.glue.GlueAuthState
import dev.bridgething.glue.GlueCapability
import dev.bridgething.glue.GlueDebugState
import dev.bridgething.glue.GlueError
import dev.bridgething.glue.GlueNowPlaying
import dev.bridgething.glue.GlueServiceHealth
import dev.bridgething.schema.AuthorityClaim
import dev.bridgething.schema.AuthorityRelease
import dev.bridgething.schema.BrowseEntry
import dev.bridgething.schema.BrowseFolder
import dev.bridgething.schema.BrowseResult
import dev.bridgething.schema.CompanionAuthorityScope
import dev.bridgething.schema.FavoritesPage
import dev.bridgething.schema.FavoritesSet
import dev.bridgething.schema.ItemKind
import dev.bridgething.schema.ItemRef
import dev.bridgething.schema.LibraryBrowseRequest
import dev.bridgething.schema.LibraryFavoritesContainsRequest
import dev.bridgething.schema.LibraryFavoritesListRequest
import dev.bridgething.schema.LibraryItem
import dev.bridgething.schema.LibraryRecommendationsRequest
import dev.bridgething.schema.LibrarySearchRequest
import dev.bridgething.schema.MediaItemUpdate
import dev.bridgething.schema.MusicProvider
import dev.bridgething.schema.NowPlayingUpdate
import dev.bridgething.schema.PlayUri
import dev.bridgething.schema.PlaybackHint
import dev.bridgething.schema.PlaybackUpdate
import dev.bridgething.schema.QueuePosition
import dev.bridgething.schema.QueueUri
import dev.bridgething.schema.RecommendationsResult
import dev.bridgething.schema.SearchResult
import dev.bridgething.schema.ShuffleMode
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import io.ktor.client.HttpClient
import io.ktor.client.engine.cio.CIO
import io.ktor.client.request.get
import io.ktor.client.statement.HttpResponse
import io.ktor.client.statement.bodyAsBytes
import io.ktor.http.HttpHeaders
import java.net.URLDecoder
import java.net.URLEncoder
import dev.bridgething.schema.Album as WireAlbum
import dev.bridgething.schema.Artist as WireArtist
import dev.bridgething.schema.Playlist as WirePlaylist
import dev.bridgething.schema.PodcastEpisode as WirePodcastEpisode
import dev.bridgething.schema.RepeatMode as WireRepeat
import dev.bridgething.schema.Show as WireShow
import dev.bridgething.schema.Track as WireTrack

private const val ASSET_ID_PREFIX = "spotify/img/"
private const val HINT_DEBOUNCE_MS = 250L
private const val POLL_INTERVAL_MS = 60_000L
private const val SPOTIFY_APP_BUNDLE = "com.spotify.client"

typealias SpotifyAuthenticatorFactory = () -> SpotifyAuthenticator

/** `BridgethingGlue` impl; no dealer websocket, so now-playing is driven by playback hints plus a baseline poll. */
class SpotifyGlue(
    private val authenticatorFactory: SpotifyAuthenticatorFactory,
    private val accessToken: String = "",
    private val refreshToken: String = "",
    private val onTokensRefreshed: ((accessToken: String, refreshToken: String) -> Unit)? = null,
) : BridgethingGlue, SpotinyDelegate {
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

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val httpClient = HttpClient(CIO)

    private var client: SpotinyClient? = null
    private var gateway: BridgethingGateway? = null
    private var authorityHeld: Boolean = false
    private var nowPlayingObserver: ((GlueNowPlaying?) -> Unit)? = null
    private var authObserver: ((GlueAuthState) -> Unit)? = null
    private var serviceHealthObserver: ((GlueServiceHealth) -> Unit)? = null
    private var hintFetchJob: Job? = null
    private var baselinePollJob: Job? = null
    private var connectJob: Job? = null

    override suspend fun attach(gateway: BridgethingGateway) {
        if (this.gateway != null) detach()

        this.gateway = gateway

        if (accessToken.isEmpty() && refreshToken.isEmpty()) {
            // no tokens yet: RN drives interactive sign-in and re-attaches via completeSpotifySignIn.
            authObserver?.invoke(GlueAuthState.Pending(null))
            return
        }
        // optimistic: tokens present means we were signed in, so show authenticated up-front;
        // the background refresh downgrades to failed only if it actually fails.
        authObserver?.invoke(GlueAuthState.Authenticated)

        val client = SpotinyClient(
            authenticator = authenticatorFactory(),
            delegate = this,
            accessToken = accessToken,
            refreshToken = refreshToken,
        )
        this.client = client

        connectJob = scope.launch { client.connect() }
    }

    override suspend fun detach() {
        authObserver = null
        serviceHealthObserver = null

        connectJob?.cancel()
        connectJob = null
        hintFetchJob?.cancel()
        hintFetchJob = null
        baselinePollJob?.cancel()
        baselinePollJob = null

        val gw = gateway
        if (gw != null && authorityHeld) {
            runCatching { gw.authority.release(AuthorityRelease(CompanionAuthorityScope.NowPlayingPlayback)) }
            runCatching { gw.authority.release(AuthorityRelease(CompanionAuthorityScope.NowPlayingMetadata)) }
        }
        authorityHeld = false

        nowPlayingObserver?.invoke(null)
        nowPlayingObserver = null

        client = null
        gateway = null
    }

    override suspend fun setNowPlayingObserver(observer: (GlueNowPlaying?) -> Unit) {
        nowPlayingObserver = observer
    }

    override suspend fun setAuthObserver(observer: (GlueAuthState) -> Unit) {
        authObserver = observer
    }

    override suspend fun setServiceHealthObserver(observer: (GlueServiceHealth) -> Unit) {
        serviceHealthObserver = observer
        observer(GlueServiceHealth.Ok)
    }

    override suspend fun play(uri: PlayUri) {
        val client = client ?: throw GlueError.Detached
        val context = uri.context
        if (context != null) {
            val parsed = SpotifyUri.parse(context.contextUri) ?: throw GlueError.NotImplemented
            val skip = SpotifyUri.parse(uri.uri)
            client.player.play(uri = parsed, skipToUri = skip)
        } else {
            val parsed = SpotifyUri.parse(uri.uri) ?: throw GlueError.NotImplemented
            client.player.play(uri = parsed)
        }
    }

    suspend fun queue(req: QueueUri) {
        val client = client ?: throw GlueError.Detached
        if (req.position is QueuePosition.Index) throw GlueError.NotImplemented
        val parsed = SpotifyUri.parse(req.uri) ?: throw GlueError.NotImplemented
        client.player.addItemToQueue(parsed)
    }

    override suspend fun pause() {
        val client = client ?: throw GlueError.Detached
        client.player.pause()
    }

    override suspend fun resume() {
        val client = client ?: throw GlueError.Detached
        client.player.resume()
    }

    override suspend fun skipNext() {
        val client = client ?: throw GlueError.Detached
        client.player.skipNext()
    }

    override suspend fun skipPrev() {
        val client = client ?: throw GlueError.Detached
        client.player.skipPrevious()
    }

    override suspend fun seekTo(positionMs: UInt) {
        val client = client ?: throw GlueError.Detached
        client.player.seek(positionMs.toInt())
    }

    override suspend fun setShuffle(on: Boolean) {
        val client = client ?: throw GlueError.Detached
        client.player.setShuffle(on)
    }

    override suspend fun setRepeat(mode: WireRepeat) {
        val client = client ?: throw GlueError.Detached
        val mapped = when (mode) {
            WireRepeat.Off -> RepeatMode.OFF
            WireRepeat.All -> RepeatMode.CONTEXT
            WireRepeat.One -> RepeatMode.TRACK
        }
        client.player.setRepeatMode(mapped)
    }

    override suspend fun search(req: LibrarySearchRequest): SearchResult {
        val client = client ?: throw GlueError.Detached

        val kinds = req.kinds?.takeIf { it.isNotEmpty() }
            ?: listOf(ItemKind.Track, ItemKind.Album, ItemKind.Artist, ItemKind.Playlist)
        val types = kinds.mapNotNull { spotifyType(it) }
        if (types.isEmpty()) {
            return SearchResult(items = emptyList(), kinds = emptyList(), total = null, hasMore = false)
        }

        val results = client.search.search(
            query = req.query, types = types, limit = req.limit.toInt(), offset = req.offset.toInt(),
        )

        val limit = req.limit.toInt()
        val items = mutableListOf<LibraryItem>()
        val presentKinds = mutableListOf<ItemKind>()
        var reachedFullPage = false

        for (kind in kinds) {
            val kindItems: List<LibraryItem> = when (kind) {
                ItemKind.Track -> results.tracks.map { LibraryItem.Track(mapTrack(it)) }
                ItemKind.Album -> results.albums.map { LibraryItem.Album(mapAlbum(it)) }
                ItemKind.Artist -> results.artists.map { LibraryItem.Artist(mapArtist(it)) }
                ItemKind.Playlist -> results.playlists.map { LibraryItem.Playlist(mapPlaylist(it)) }
                ItemKind.Show -> results.shows.map { LibraryItem.Show(mapShow(it)) }
                ItemKind.PodcastEpisode -> results.episodes.map { LibraryItem.PodcastEpisode(mapEpisode(it)) }
                ItemKind.Station -> emptyList()
            }
            if (kindItems.isNotEmpty()) {
                presentKinds.add(kind)
                if (kindItems.size >= limit) reachedFullPage = true
            }
            items.addAll(kindItems)
        }

        return SearchResult(items = items, kinds = presentKinds, total = null, hasMore = reachedFullPage)
    }

    override suspend fun browse(req: LibraryBrowseRequest): BrowseResult {
        val client = client ?: throw GlueError.Detached
        val limit = req.limit.toInt()
        val offset = req.offset.toInt()

        return when (req.nodeId) {
            null, "", "root" -> browseRoot(client)

            RECENTLY_PLAYED_NODE -> {
                val entries = dedupedTrackEntries(client.player.getRecentlyPlayed(50))
                BrowseResult(entries = entries, total = entries.size.toUInt(), hasMore = false)
            }

            TOP_TRACKS_NODE -> {
                val page = client.tracks.getUserTopTracks(limit, offset)
                val entries = page.items.map { BrowseEntry.Item(LibraryItem.Track(mapTrack(it))) }
                section(entries, page.items.size, page.total, offset)
            }

            HOME_NODE -> {
                val page = client.categories.getMadeForYou(limit, offset)
                val entries = page.items.map { BrowseEntry.Item(LibraryItem.Playlist(mapPlaylist(it))) }
                section(entries, page.items.size, page.total, offset)
            }

            PLAYLISTS_NODE -> {
                val page = client.playlists.getUserPlaylists(limit, offset)
                val entries = mutableListOf<BrowseEntry>()
                if (offset == 0) {
                    val userId = client.users.getCurrentUser()?.id
                    likedSongsEntry(userId)?.let { entries.add(it) }
                }
                entries += page.items.map { BrowseEntry.Item(LibraryItem.Playlist(mapPlaylist(it))) }
                section(entries, page.items.size, page.total, offset)
            }

            PODCASTS_NODE -> {
                val page = client.shows.getUserSavedShows(limit, offset)
                val entries = mutableListOf<BrowseEntry>()
                if (offset == 0) entries.add(yourEpisodesEntry())
                entries += page.items.map { BrowseEntry.Item(LibraryItem.Show(mapShow(it))) }
                section(entries, page.items.size, page.total, offset)
            }

            ARTISTS_NODE -> {
                val page = client.artists.getUserFollowedArtists(limit, offset)
                val entries = page.items.map { BrowseEntry.Item(LibraryItem.Artist(mapArtist(it))) }
                section(entries, page.items.size, page.total, offset)
            }

            ALBUMS_NODE -> {
                val page = client.albums.getUserSavedAlbums(limit, offset)
                val entries = page.items.map { BrowseEntry.Item(LibraryItem.Album(mapAlbum(it))) }
                section(entries, page.items.size, page.total, offset)
            }

            else -> BrowseResult(entries = emptyList(), total = null, hasMore = false)
        }
    }

    private suspend fun browseRoot(client: SpotinyClient): BrowseResult = coroutineScope {
        val previewLimit = 14

        val recentP = async { client.player.getRecentlyPlayed(previewLimit) }
        val topP = async { client.tracks.getUserTopTracks(previewLimit) }
        val homeP = async { client.categories.getMadeForYou(10) }
        val playlistsP = async { client.playlists.getUserPlaylists(previewLimit) }
        val showsP = async { client.shows.getUserSavedShows(previewLimit) }
        val artistsP = async { client.artists.getUserFollowedArtists(previewLimit) }
        val albumsP = async { client.albums.getUserSavedAlbums(previewLimit) }
        val userP = async { client.users.getCurrentUser() }

        val recentChildren = dedupedTrackEntries(recentP.await())
        val topChildren = topP.await().items.map { BrowseEntry.Item(LibraryItem.Track(mapTrack(it))) }
        val homeChildren = homeP.await().items.map { BrowseEntry.Item(LibraryItem.Playlist(mapPlaylist(it))) }

        val playlistChildren = mutableListOf<BrowseEntry>()
        likedSongsEntry(userP.await()?.id)?.let { playlistChildren.add(it) }
        playlistChildren += playlistsP.await().items.map { BrowseEntry.Item(LibraryItem.Playlist(mapPlaylist(it))) }

        val podcastChildren = mutableListOf<BrowseEntry>(yourEpisodesEntry())
        podcastChildren += showsP.await().items.map { BrowseEntry.Item(LibraryItem.Show(mapShow(it))) }

        val artistChildren = artistsP.await().items.map { BrowseEntry.Item(LibraryItem.Artist(mapArtist(it))) }
        val albumChildren = albumsP.await().items.map { BrowseEntry.Item(LibraryItem.Album(mapAlbum(it))) }

        val folders = mutableListOf<BrowseEntry>()
        fun addSection(nodeId: String, title: String, children: List<BrowseEntry>, total: UInt?) {
            if (children.isEmpty()) return
            folders.add(
                BrowseEntry.Folder(
                    BrowseFolder(
                        nodeId = nodeId, title = title, subtitle = null, artworkId = null,
                        total = total, previewChildren = children,
                    ),
                ),
            )
        }
        addSection(RECENTLY_PLAYED_NODE, "Recently Played", recentChildren, recentChildren.size.toUInt())
        addSection(TOP_TRACKS_NODE, "Top Tracks", topChildren, topChildren.size.toUInt())
        addSection(HOME_NODE, "Home", homeChildren, homeChildren.size.toUInt())
        addSection(PLAYLISTS_NODE, "Playlists", playlistChildren, null)
        addSection(PODCASTS_NODE, "Podcasts", podcastChildren, null)
        addSection(ARTISTS_NODE, "Artists", artistChildren, null)
        addSection(ALBUMS_NODE, "Albums", albumChildren, null)
        BrowseResult(entries = folders, total = folders.size.toUInt(), hasMore = false)
    }

    override suspend fun recommendations(req: LibraryRecommendationsRequest): RecommendationsResult {
        val client = client ?: throw GlueError.Detached
        val limit = req.limit.toInt()

        val seedTracks = req.seeds.filter { it.kind == ItemKind.Track }.mapNotNull { SpotifyUri.parse(it.uri)?.id }
        val seedArtists = req.seeds.filter { it.kind == ItemKind.Artist }.mapNotNull { SpotifyUri.parse(it.uri)?.id }

        val tracks = client.recommendations.get(seedTracks = seedTracks, seedArtists = seedArtists, limit = limit)
        if (tracks.isNotEmpty()) {
            return RecommendationsResult(
                items = tracks.map { LibraryItem.Track(mapTrack(it)) }, total = null, hasMore = false,
            )
        }

        val artistSeed = req.seeds.firstOrNull { it.kind == ItemKind.Artist }
        val parsed = artistSeed?.let { SpotifyUri.parse(it.uri) }
        if (parsed != null) {
            val top = client.artists.getArtistTopTracks(parsed).take(limit)
            return RecommendationsResult(
                items = top.map { LibraryItem.Track(mapTrack(it)) }, total = null, hasMore = false,
            )
        }

        return RecommendationsResult(items = emptyList(), total = null, hasMore = false)
    }

    override suspend fun favoritesList(req: LibraryFavoritesListRequest): FavoritesPage {
        val client = client ?: throw GlueError.Detached
        val offset = req.offset.toInt()
        val page = client.tracks.getUserSavedTracks(req.limit.toInt(), offset)
        val items = page.items.map { LibraryItem.Track(mapTrack(it, saved = true)) }
        return FavoritesPage(
            items = items,
            total = maxOf(page.total, 0).toUInt(),
            hasMore = offset + page.items.size < page.total,
        )
    }

    override suspend fun favoritesContains(req: LibraryFavoritesContainsRequest): List<Boolean> {
        val client = client ?: throw GlueError.Detached
        return client.library.contains(req.uris)
    }

    override suspend fun favoritesToggle(item: ItemRef) {
        val client = client ?: throw GlueError.Detached
        val uri = SpotifyUri.parse(item.uri) ?: throw GlueError.NotImplemented
        val saved = client.library.contains(listOf(item.uri)).firstOrNull() ?: false
        if (saved) {
            client.library.remove(listOf(uri))
        } else {
            client.library.save(listOf(uri))
        }
    }

    override suspend fun favoritesSet(item: ItemRef, liked: Boolean) {
        val client = client ?: throw GlueError.Detached
        val uri = SpotifyUri.parse(item.uri) ?: throw GlueError.NotImplemented
        if (liked) {
            client.library.save(listOf(uri))
        } else {
            client.library.remove(listOf(uri))
        }
    }

    override suspend fun favoritesSetMany(entries: List<FavoritesSet>) {
        val client = client ?: throw GlueError.Detached
        val toSave = entries.filter { it.liked }.mapNotNull { SpotifyUri.parse(it.item.uri) }
        val toRemove = entries.filter { !it.liked }.mapNotNull { SpotifyUri.parse(it.item.uri) }
        if (toSave.isNotEmpty()) client.library.save(toSave)
        if (toRemove.isNotEmpty()) client.library.remove(toRemove)
    }

    suspend fun handlePlaybackHint(hint: PlaybackHint) {
        if (hint.appBundle != SPOTIFY_APP_BUNDLE) return

        DiagnosticsBuffer.recordBreadcrumb(
            category = "spotify.merge",
            detail = "iap2 playback hint",
            fields = listOf("source" to "iap2BackProp"),
        )
        hintFetchJob?.cancel()
        hintFetchJob = scope.launch {
            delay(HINT_DEBOUNCE_MS)
            if (!isActive) return@launch
            fetchAndDispatch("hint")
        }
    }

    override suspend fun debugState(): GlueDebugState = GlueDebugState(
        authorityPlaybackHeld = authorityHeld,
        authorityMetadataHeld = authorityHeld,
        baselinePollActive = baselinePollJob != null,
        hintFetchActive = hintFetchJob != null,
    )

    override suspend fun asset(id: String): AssetBytes? {
        if (!id.startsWith(ASSET_ID_PREFIX)) return null
        val encoded = id.substring(ASSET_ID_PREFIX.length)
        val urlString = runCatching { URLDecoder.decode(encoded, "UTF-8") }.getOrNull() ?: return null
        val response: HttpResponse = httpClient.get(urlString)
        if (response.status.value !in 200..299) return null
        val mime = response.headers[HttpHeaders.ContentType]
        return AssetBytes(bytes = response.bodyAsBytes(), mime = mime)
    }

    private suspend fun fetchAndDispatch(reason: String) {
        val client = client ?: return
        val state = client.player.getPlaybackState() ?: return
        handleStateUpdate(state, reason)
    }

    private fun handleStateUpdate(state: PlayerState, reason: String) {
        val gateway = gateway ?: return
        val update = makeUpdate(state)
        val artworkUrl = state.item?.let { rawArtworkUrl(it) }
        nowPlayingObserver?.invoke(GlueNowPlaying(update = update, artworkUrl = artworkUrl))

        val nowPlaying = state.isPlaying
        DiagnosticsBuffer.recordBreadcrumb(
            category = "spotify.merge",
            detail = "augmented now-playing",
            fields = listOf(
                "source" to "companionAugmented",
                "reason" to reason,
                "track" to (state.item?.name ?: ""),
                "playing" to nowPlaying.toString(),
            ),
        )
        scope.launch {
            runCatching { gateway.player.delta(update) }
            if (nowPlaying) {
                runCatching { gateway.authority.claim(AuthorityClaim(CompanionAuthorityScope.NowPlayingPlayback)) }
                runCatching { gateway.authority.claim(AuthorityClaim(CompanionAuthorityScope.NowPlayingMetadata)) }
                authorityHeld = true
                DiagnosticsBuffer.recordBreadcrumb("spotify.authority", "claimed playback+metadata")
                startBaselinePollIfNeeded()
            } else if (authorityHeld) {
                runCatching { gateway.authority.release(AuthorityRelease(CompanionAuthorityScope.NowPlayingPlayback)) }
                runCatching { gateway.authority.release(AuthorityRelease(CompanionAuthorityScope.NowPlayingMetadata)) }
                authorityHeld = false
                DiagnosticsBuffer.recordBreadcrumb("spotify.authority", "released playback+metadata")
                stopBaselinePoll()
            }
        }
    }

    private fun handleSocketDown() {
        nowPlayingObserver?.invoke(null)
        stopBaselinePoll()
        val gateway = gateway ?: return
        if (!authorityHeld) return
        authorityHeld = false
        scope.launch {
            runCatching { gateway.authority.release(AuthorityRelease(CompanionAuthorityScope.NowPlayingPlayback)) }
            runCatching { gateway.authority.release(AuthorityRelease(CompanionAuthorityScope.NowPlayingMetadata)) }
        }
    }

    private fun startBaselinePollIfNeeded() {
        if (baselinePollJob != null) return
        baselinePollJob = scope.launch {
            while (isActive) {
                delay(POLL_INTERVAL_MS)
                if (!isActive) return@launch
                fetchAndDispatch("poll")
            }
        }
    }

    private fun stopBaselinePoll() {
        baselinePollJob?.cancel()
        baselinePollJob = null
    }

    override fun authDidRefresh(accessToken: String, refreshToken: String) {
        onTokensRefreshed?.invoke(accessToken, refreshToken)
        if (accessToken.isNotEmpty()) {
            authObserver?.invoke(GlueAuthState.Authenticated)
        }
    }

    override fun authDidFail(reason: String) {
        handleSocketDown()
        authObserver?.invoke(GlueAuthState.Failed(reason))
    }

    override fun socketDidConnect() {}

    override fun socketDidDisconnect() {
        handleSocketDown()
    }

    override fun playerStateUpdated(oldState: PlayerState?, newState: PlayerState) {
        handleStateUpdate(newState, "dealer")
    }

    override fun serviceDidRateLimit(retryAfterSeconds: Int) {
        serviceHealthObserver?.invoke(GlueServiceHealth.RateLimited(retryAfterSeconds))
    }

    override fun serviceDidRecover() {
        serviceHealthObserver?.invoke(GlueServiceHealth.Ok)
    }

    private companion object {
        const val RECENTLY_PLAYED_NODE = "recently-played"
        const val TOP_TRACKS_NODE = "top-tracks"
        const val HOME_NODE = "home"
        const val PLAYLISTS_NODE = "playlists"
        const val PODCASTS_NODE = "podcasts"
        const val ARTISTS_NODE = "artists"
        const val ALBUMS_NODE = "albums"

        fun section(entries: List<BrowseEntry>, pageCount: Int, total: Int, offset: Int): BrowseResult =
            BrowseResult(
                entries = entries,
                total = maxOf(total, 0).toUInt(),
                hasMore = offset + pageCount < total,
            )

        fun dedupedTrackEntries(tracks: List<Track>): List<BrowseEntry> {
            val seen = mutableSetOf<String>()
            return tracks.mapNotNull { track ->
                if (!seen.add(track.uri)) return@mapNotNull null
                BrowseEntry.Item(LibraryItem.Track(mapTrack(track)))
            }
        }

        fun likedSongsEntry(userId: String?): BrowseEntry? {
            if (userId == null) return null
            val uri = SpotifyUri.build(SpotifyUri.Kind.COLLECTION, userId)
            return BrowseEntry.Item(
                LibraryItem.Playlist(
                    WirePlaylist(
                        uri = uri.string(), name = "Liked Songs", ownerName = null, trackCount = null, artworkId = null,
                    ),
                ),
            )
        }

        fun yourEpisodesEntry(): BrowseEntry =
            BrowseEntry.Item(
                LibraryItem.Playlist(
                    WirePlaylist(
                        uri = SpotifyUri.Static.YOUR_EPISODES, name = "Your Episodes",
                        ownerName = null, trackCount = null, artworkId = null,
                    ),
                ),
            )

        fun makeUpdate(state: PlayerState): NowPlayingUpdate {
            val media: MediaItemUpdate? = state.item?.let { item ->
                val title = item.name
                val artist = item.artists.joinToString(", ") { it.name }
                val album = (item as? PlayerItem.TrackItem)?.track?.album?.name
                MediaItemUpdate(
                    persistentId = item.uri,
                    title = title.ifEmpty { null },
                    album = album,
                    albumArtist = null,
                    artist = artist.ifEmpty { null },
                    liked = null,
                    artworkId = artworkId(item),
                    durationMs = maxOf(item.durationMs, 0).toUInt(),
                    mediaTypes = null,
                    trackNumber = null,
                    trackCount = null,
                    isLikeSupported = null,
                    isBanSupported = null,
                    isBanned = null,
                    isResidentOnDevice = null,
                    chapterCount = null,
                )
            }

            val allowSeek = state.actions?.disallows?.seeking?.let { !it } ?: true
            val playback = PlaybackUpdate(
                playing = state.isPlaying,
                positionMs = maxOf(state.progressMs, 0).toUInt(),
                shuffle = state.shuffleState,
                shuffleMode = if (state.shuffleState) ShuffleMode.Songs else ShuffleMode.Off,
                repeat = mapRepeat(state.repeatState),
                appBundle = SPOTIFY_APP_BUNDLE,
                appDisplayName = "Spotify",
                queueIndex = null,
                queueCount = null,
                queueChapterIndex = null,
                playbackSpeed = null,
                setElapsedTimeAvailable = allowSeek,
                queueListAvail = null,
                appleMusicRadioAd = null,
                appleMusicRadioStationName = null,
            )

            return NowPlayingUpdate(mediaItem = media, playback = playback)
        }

        fun artworkId(item: PlayerItem): String? = imageAssetId(rawArtworkUrl(item) ?: "")

        fun rawArtworkUrl(item: PlayerItem): String? {
            val url = bestImageUrl(item.imageUrl)
            return url.ifEmpty { null }
        }

        fun bestImageUrl(urls: SpotifyImageURLs): String {
            if (urls.large.isNotEmpty()) return urls.large
            if (urls.medium.isNotEmpty()) return urls.medium
            if (urls.small.isNotEmpty()) return urls.small
            return ""
        }

        fun imageAssetId(rawUrl: String): String? {
            if (rawUrl.isEmpty()) return null
            val encoded = URLEncoder.encode(rawUrl, "UTF-8")
                .replace("+", "%20")
                .replace("*", "%2A")
                .replace("%7E", "~")
            return ASSET_ID_PREFIX + encoded
        }

        fun spotifyType(kind: ItemKind): String? = when (kind) {
            ItemKind.Track -> "track"
            ItemKind.Album -> "album"
            ItemKind.Artist -> "artist"
            ItemKind.Playlist -> "playlist"
            ItemKind.Show -> "show"
            ItemKind.PodcastEpisode -> "episode"
            ItemKind.Station -> null
        }

        fun mapTrack(t: Track, saved: Boolean = false): WireTrack {
            val primary = t.artists.firstOrNull()
            return WireTrack(
                id = t.uri,
                name = t.name,
                album = WireAlbum(id = t.album?.uri ?: "", name = t.album?.name ?: ""),
                artist = WireArtist(id = primary?.uri ?: "", name = primary?.name ?: ""),
                artists = t.artists.map { WireArtist(id = it.uri, name = it.name) },
                duration_ms = maxOf(t.durationMs, 0).toUInt(),
                image_id = imageAssetId(bestImageUrl(t.imageUrl)) ?: "",
                saved = saved,
            )
        }

        fun mapAlbum(a: Album): WireAlbum = WireAlbum(id = a.uri, name = a.name)

        fun mapArtist(a: Artist): WireArtist = WireArtist(id = a.uri, name = a.name)

        fun mapPlaylist(p: Playlist): WirePlaylist = WirePlaylist(
            uri = p.uri,
            name = p.name,
            ownerName = null,
            trackCount = null,
            artworkId = imageAssetId(bestImageUrl(p.imageUrl)),
        )

        fun mapShow(s: Show): WireShow = WireShow(
            uri = s.uri,
            name = s.name,
            publisher = null,
            episodeCount = null,
            artworkId = imageAssetId(bestImageUrl(s.imageUrl)),
        )

        fun mapEpisode(e: Episode): WirePodcastEpisode = WirePodcastEpisode(
            uri = e.uri,
            name = e.name,
            showName = e.show?.name,
            durationMs = maxOf(e.durationMs, 0).toUInt(),
            publishedAtUnixS = null,
            artworkId = imageAssetId(bestImageUrl(e.imageUrl)),
        )

        fun mapRepeat(mode: RepeatMode): WireRepeat = when (mode) {
            RepeatMode.OFF -> WireRepeat.Off
            RepeatMode.TRACK -> WireRepeat.One
            RepeatMode.CONTEXT -> WireRepeat.All
        }
    }
}
