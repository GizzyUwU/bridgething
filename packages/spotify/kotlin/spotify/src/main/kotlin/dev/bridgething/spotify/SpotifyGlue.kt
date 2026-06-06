package dev.bridgething.spotify

import dev.bridgething.gateway.BridgethingGateway
import dev.bridgething.gateway.asset
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
import dev.bridgething.schema.ContextResolveReply
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
import dev.bridgething.schema.MediaItem
import dev.bridgething.schema.MediaItemUpdate
import dev.bridgething.schema.MusicProvider
import dev.bridgething.schema.NowPlayingUpdate
import dev.bridgething.schema.Playback
import dev.bridgething.schema.PlaybackContext
import dev.bridgething.schema.PlaybackState
import dev.bridgething.schema.PlayerOptions
import dev.bridgething.schema.PlayUri
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
import io.ktor.client.HttpClientConfig
import io.ktor.client.engine.HttpClientEngine
import io.ktor.client.engine.cio.CIO
import io.ktor.client.plugins.HttpTimeout
import io.ktor.client.request.get
import io.ktor.client.statement.HttpResponse
import io.ktor.client.statement.bodyAsBytes
import java.net.URLDecoder
import java.net.URLEncoder
import dev.bridgething.schema.Album as WireAlbum
import dev.bridgething.schema.Artist as WireArtist
import dev.bridgething.schema.Playlist as WirePlaylist
import dev.bridgething.schema.PlayerState as WirePlayerState
import dev.bridgething.schema.PodcastEpisode as WirePodcastEpisode
import dev.bridgething.schema.RepeatMode as WireRepeat
import dev.bridgething.schema.Show as WireShow
import dev.bridgething.schema.Track as WireTrack

private const val ASSET_ID_PREFIX = "spotify/img/"
private const val SCDN_IMAGE_PREFIX = "https://i.scdn.co/image/"
private const val DEFAULT_HERO_EDGE = 248
private const val DEFAULT_THUMB_EDGE = 96
private const val SPOTIFY_APP_BUNDLE = "com.spotify.client"

typealias SpotifyAuthenticatorFactory = () -> SpotifyAuthenticator

/** `BridgethingGlue` impl. */
class SpotifyGlue(
    private val authenticatorFactory: SpotifyAuthenticatorFactory,
    private val accessToken: String = "",
    private val refreshToken: String = "",
    private val onTokensRefreshed: ((accessToken: String, refreshToken: String) -> Unit)? = null,
    private val engine: HttpClientEngine? = null,
    cacheDir: java.io.File? = null,
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
    private val httpClient = run {
        val configure: HttpClientConfig<*>.() -> Unit = {
            install(HttpTimeout) {
                requestTimeoutMillis = 6_000
                connectTimeoutMillis = 4_000
            }
        }
        if (engine != null) HttpClient(engine, configure) else HttpClient(CIO, configure)
    }

    private val imageCache: ImageDiskCache? =
        cacheDir?.let { ImageDiskCache(java.io.File(it, "spotify-art"), 200L shl 20) }

    private var client: SpotinyClient? = null
    private var gateway: BridgethingGateway? = null
    private var authorityHeld: Boolean = false
    private var nowPlayingObserver: ((GlueNowPlaying?) -> Unit)? = null
    private var authObserver: ((GlueAuthState) -> Unit)? = null
    private var serviceHealthObserver: ((GlueServiceHealth) -> Unit)? = null
    private var connectJob: Job? = null
    @Volatile private var heroEdge: Int = DEFAULT_HERO_EDGE
    @Volatile private var thumbEdge: Int = DEFAULT_THUMB_EDGE
    private val likedByUri = java.util.concurrent.ConcurrentHashMap<String, Boolean>()
    @Volatile private var lastSnapshotState: PlayerState? = null

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
            engine = engine,
        )
        this.client = client

        connectJob = scope.launch {
            client.connect()
        }
    }

    override suspend fun detach() {
        authObserver = null
        serviceHealthObserver = null

        connectJob?.cancel()
        connectJob = null
        runCatching { client?.disconnect() }

        val gw = gateway
        if (gw != null && authorityHeld) {
            runCatching { gw.authority.release(AuthorityRelease(CompanionAuthorityScope.NowPlayingPlayback)) }
            runCatching { gw.authority.release(AuthorityRelease(CompanionAuthorityScope.NowPlayingMetadata)) }
        }
        authorityHeld = false

        nowPlayingObserver?.invoke(null)
        nowPlayingObserver = null

        likedByUri.clear()
        lastSnapshotState = null
        client = null
        gateway = null
    }

    override suspend fun setNowPlayingObserver(observer: (GlueNowPlaying?) -> Unit) {
        nowPlayingObserver = observer
    }

    override suspend fun setArtProfile(heroPx: Int, thumbPx: Int) {
        heroEdge = heroPx.coerceAtLeast(1)
        thumbEdge = thumbPx.coerceAtLeast(1)
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
        }    }

    override suspend fun queue(req: QueueUri) {
        val client = client ?: throw GlueError.Detached
        if (req.position is QueuePosition.Index) throw GlueError.NotImplemented
        val parsed = SpotifyUri.parse(req.uri) ?: throw GlueError.NotImplemented
        client.player.addItemToQueue(parsed)
    }

    override suspend fun pause() {
        val client = client ?: throw GlueError.Detached
        client.player.pause()    }

    override suspend fun resume() {
        val client = client ?: throw GlueError.Detached
        client.player.resume()    }

    override suspend fun skipNext() {
        val client = client ?: throw GlueError.Detached
        client.player.skipNext()    }

    override suspend fun skipPrev() {
        val client = client ?: throw GlueError.Detached
        client.player.skipPrevious()    }

    override suspend fun seekTo(positionMs: UInt) {
        val client = client ?: throw GlueError.Detached
        client.player.seek(positionMs.toInt())    }

    override suspend fun setShuffle(on: Boolean) {
        val client = client ?: throw GlueError.Detached
        client.player.setShuffle(on)    }

    override suspend fun setRepeat(mode: WireRepeat) {
        val client = client ?: throw GlueError.Detached
        val mapped = when (mode) {
            WireRepeat.Off -> RepeatMode.OFF
            WireRepeat.All -> RepeatMode.CONTEXT
            WireRepeat.One -> RepeatMode.TRACK
        }
        client.player.setRepeatMode(mapped)    }

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

    override suspend fun resolveContext(uri: String): ContextResolveReply {
        val client = client ?: throw GlueError.Detached
        val parsed = SpotifyUri.parse(uri) ?: throw GlueError.NotImplemented
        fun reply(name: String?, imageUrl: SpotifyImageURLs?, subtitle: String? = null): ContextResolveReply {
            val artworkId = imageUrl?.let { imageAssetId(bestImageUrl(it, heroEdge), heroEdge) }
            return ContextResolveReply(name = name, artworkId = artworkId, subtitle = subtitle)
        }
        return when (parsed.kind) {
            SpotifyUri.Kind.PLAYLIST, SpotifyUri.Kind.PLAYLIST_V2 -> {
                val p = client.playlists.getPlaylist(parsed)
                reply(p?.name, p?.imageUrl)
            }
            SpotifyUri.Kind.ALBUM -> {
                val a = client.albums.getAlbum(parsed)
                reply(a?.name, a?.imageUrl, a?.artists?.firstOrNull()?.name)
            }
            SpotifyUri.Kind.SHOW -> {
                val s = client.shows.getShow(parsed)
                reply(s?.name, s?.imageUrl)
            }
            SpotifyUri.Kind.ARTIST, SpotifyUri.Kind.ARTIST_TOPLIST -> {
                val ar = client.artists.getArtist(parsed)
                reply(ar?.name, ar?.imageUrl)
            }
            else -> throw GlueError.NotImplemented
        }
    }

    override suspend fun browse(req: LibraryBrowseRequest): BrowseResult {
        val client = client ?: throw GlueError.Detached
        val limit = req.limit.toInt()
        val offset = req.offset.toInt()

        val result = when (req.nodeId) {
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

            else -> {
                val nodeId = req.nodeId ?: return BrowseResult(entries = emptyList(), total = null, hasMore = false)
                val parsed = SpotifyUri.parse(nodeId)
                    ?: return BrowseResult(entries = emptyList(), total = null, hasMore = false)
                browseChildren(client, parsed, limit, offset)
            }
        }
        warmArt(result)
        return result
    }

    private suspend fun browseChildren(client: SpotinyClient, uri: SpotifyUri, limit: Int, offset: Int): BrowseResult {
        return when (uri.kind) {
            SpotifyUri.Kind.PLAYLIST, SpotifyUri.Kind.PLAYLIST_V2 -> {
                val page = client.playlists.getPlaylistItems(uri, limit, offset)
                val entries = page.items.map { mapPlaylistItem(it) }
                section(entries, page.items.size, page.total, offset)
            }

            SpotifyUri.Kind.ALBUM -> {
                val page = client.albums.getAlbumTracks(uri, limit, offset)
                val entries = page.items.map { BrowseEntry.Item(LibraryItem.Track(mapTrack(it))) }
                section(entries, page.items.size, page.total, offset)
            }

            SpotifyUri.Kind.ARTIST, SpotifyUri.Kind.ARTIST_TOPLIST -> {
                // artist top-tracks is not offset-paged.
                val entries = client.artists.getArtistTopTracks(uri).map { BrowseEntry.Item(LibraryItem.Track(mapTrack(it))) }
                BrowseResult(entries = entries, total = entries.size.toUInt(), hasMore = false)
            }

            SpotifyUri.Kind.SHOW -> {
                val page = client.shows.getShowEpisodes(uri, limit, offset)
                val entries = page.items.map { BrowseEntry.Item(LibraryItem.PodcastEpisode(mapEpisode(it))) }
                section(entries, page.items.size, page.total, offset)
            }

            SpotifyUri.Kind.COLLECTION -> {
                val page = client.tracks.getUserSavedTracks(limit, offset)
                val entries = page.items.map { BrowseEntry.Item(LibraryItem.Track(mapTrack(it, saved = true))) }
                section(entries, page.items.size, page.total, offset)
            }

            SpotifyUri.Kind.YOUR_EPISODES -> {
                val page = client.episodes.getUserSavedEpisodes(limit, offset)
                val entries = page.items.map { BrowseEntry.Item(LibraryItem.PodcastEpisode(mapEpisode(it))) }
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
        // iap2 now-playing uris parse as valid kinds but carry non-base62 ids; the empty
        // sentinel keeps index alignment while skipping the spotify lookup for them.
        val uris = req.uris.map { if (spotifyUri(it) != null) it else "" }
        return client.library.contains(uris)
    }

    override suspend fun favoritesToggle(item: ItemRef) {
        val client = client ?: throw GlueError.Detached
        val uri = spotifyUri(item.uri) ?: throw GlueError.NotImplemented
        val saved = client.library.contains(listOf(item.uri)).firstOrNull() ?: false
        if (saved) {
            client.library.remove(listOf(uri))
        } else {
            client.library.save(listOf(uri))
        }
        applyLikedChange(item.uri, !saved)
    }

    override suspend fun favoritesSet(item: ItemRef, liked: Boolean) {
        val client = client ?: throw GlueError.Detached
        val uri = spotifyUri(item.uri) ?: throw GlueError.NotImplemented
        if (liked) {
            client.library.save(listOf(uri))
        } else {
            client.library.remove(listOf(uri))
        }
        applyLikedChange(item.uri, liked)
    }

    override suspend fun favoritesSetMany(entries: List<FavoritesSet>) {
        val client = client ?: throw GlueError.Detached
        val toSave = entries.filter { it.liked }.mapNotNull { spotifyUri(it.item.uri) }
        val toRemove = entries.filter { !it.liked }.mapNotNull { spotifyUri(it.item.uri) }
        if (toSave.isNotEmpty()) client.library.save(toSave)
        if (toRemove.isNotEmpty()) client.library.remove(toRemove)
        for (entry in entries) applyLikedChange(entry.item.uri, entry.liked)
    }

    private fun spotifyUri(raw: String): SpotifyUri? =
        SpotifyUri.parse(raw)?.takeIf { it.namespace == "spotify" }

    // browse tiles render small: use the device-advertised thumb edge, never the now-playing hero.
    private fun dedupedTrackEntries(tracks: List<Track>): List<BrowseEntry> {
        val seen = mutableSetOf<String>()
        return tracks.mapNotNull { track ->
            if (!seen.add(track.uri)) return@mapNotNull null
            BrowseEntry.Item(LibraryItem.Track(mapTrack(track)))
        }
    }

    private fun mapTrack(t: Track, saved: Boolean = false): WireTrack {
        val primary = t.artists.firstOrNull()
        return WireTrack(
            id = t.uri,
            name = t.name,
            album = WireAlbum(id = t.album?.uri ?: "", name = t.album?.name ?: ""),
            artist = WireArtist(id = primary?.uri ?: "", name = primary?.name ?: ""),
            artists = t.artists.map { WireArtist(id = it.uri, name = it.name) },
            duration_ms = maxOf(t.durationMs, 0).toUInt(),
            image_id = imageAssetId(bestImageUrl(t.imageUrl, thumbEdge), thumbEdge) ?: "",
            saved = saved,
        )
    }

    private fun mapPlaylistItem(item: PlaylistItem): BrowseEntry {
        if (item.type == "episode") {
            return BrowseEntry.Item(LibraryItem.PodcastEpisode(WirePodcastEpisode(
                uri = item.uri,
                name = item.name ?: "",
                showName = null,
                durationMs = maxOf(item.durationMs, 0).toUInt(),
                publishedAtUnixS = null,
                artworkId = imageAssetId(bestImageUrl(SpotifyImageURLs(item.images), thumbEdge), thumbEdge),
            )))
        }
        val primary = item.artists.firstOrNull()
        return BrowseEntry.Item(LibraryItem.Track(WireTrack(
            id = item.uri,
            name = item.name ?: "",
            album = WireAlbum(id = item.album?.uri ?: "", name = item.album?.name ?: ""),
            artist = WireArtist(id = primary?.uri ?: "", name = primary?.name ?: ""),
            artists = item.artists.map { WireArtist(id = it.uri, name = it.name) },
            duration_ms = maxOf(item.durationMs, 0).toUInt(),
            image_id = imageAssetId(bestImageUrl(item.imageUrl, thumbEdge), thumbEdge) ?: "",
            saved = false,
        )))
    }

    private fun mapAlbum(a: Album): WireAlbum = WireAlbum(
        id = a.uri,
        name = a.name,
        artwork_id = imageAssetId(bestImageUrl(a.imageUrl, thumbEdge), thumbEdge),
    )

    private fun mapArtist(a: Artist): WireArtist = WireArtist(
        id = a.uri,
        name = a.name,
        artwork_id = imageAssetId(bestImageUrl(a.imageUrl, thumbEdge), thumbEdge),
    )

    private fun mapPlaylist(p: Playlist): WirePlaylist = WirePlaylist(
        uri = p.uri,
        name = p.name,
        ownerName = null,
        trackCount = null,
        artworkId = imageAssetId(bestImageUrl(p.imageUrl, thumbEdge), thumbEdge),
    )

    private fun mapShow(s: Show): WireShow = WireShow(
        uri = s.uri,
        name = s.name,
        publisher = null,
        episodeCount = null,
        artworkId = imageAssetId(bestImageUrl(s.imageUrl, thumbEdge), thumbEdge),
    )

    private fun mapEpisode(e: Episode): WirePodcastEpisode = WirePodcastEpisode(
        uri = e.uri,
        name = e.name,
        showName = e.show?.name,
        durationMs = maxOf(e.durationMs, 0).toUInt(),
        publishedAtUnixS = null,
        artworkId = imageAssetId(bestImageUrl(e.imageUrl, thumbEdge), thumbEdge),
    )

    private fun likeFields(uri: String?): Pair<Boolean?, Boolean?> {
        if (uri == null || spotifyUri(uri) == null) return null to null
        return likedByUri[uri] to true
    }

    private fun buildSnapshot(state: PlayerState): WirePlayerState {
        val (liked, supported) = likeFields(state.item?.uri)
        return makeSnapshot(state, heroEdge, liked, supported)
    }

    private suspend fun resolveLiked(uri: String) {
        val liked = client?.library?.contains(listOf(uri))?.firstOrNull() ?: return
        likedByUri[uri] = liked
        reemitSnapshotIfCurrent(uri)
    }

    private suspend fun reemitSnapshotIfCurrent(uri: String) {
        val pending = lastSnapshotState ?: return
        val gw = gateway ?: return
        if (pending.item?.uri != uri) return
        runCatching { gw.player.snapshot(buildSnapshot(pending)) }
    }

    private suspend fun applyLikedChange(uri: String, liked: Boolean) {
        likedByUri[uri] = liked
        reemitSnapshotIfCurrent(uri)
    }

    override suspend fun debugState(): GlueDebugState = GlueDebugState(
        authorityPlaybackHeld = authorityHeld,
        authorityMetadataHeld = authorityHeld,
    )

    override suspend fun asset(id: String): AssetBytes? {
        val (urlString, maxEdge) = parseImageId(id) ?: return null
        val master = fetchMaster(urlString) ?: return null
        val scaled = downsample(master, maxEdge)
        return AssetBytes(bytes = scaled ?: master, mime = "image/jpeg")
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
        is LibraryItem.Album, is LibraryItem.Artist -> null
    }

    private fun handleStateUpdate(state: PlayerState) {
        val gateway = gateway ?: return
        val currentUri = state.item?.uri
        val (liked, likeSupported) = likeFields(currentUri)
        val update = makeUpdate(state, heroEdge, liked, likeSupported)
        val artworkUrl = state.item?.let { rawArtworkUrl(it, heroEdge) }
        nowPlayingObserver?.invoke(GlueNowPlaying(update = update, artworkUrl = artworkUrl))

        lastSnapshotState = state
        if (currentUri != null && spotifyUri(currentUri) != null && likedByUri[currentUri] == null) {
            scope.launch { resolveLiked(currentUri) }
        }

        val hasItem = state.item != null
        scope.launch {
            runCatching { gateway.player.snapshot(makeSnapshot(state, heroEdge, liked, likeSupported)) }
            if (hasItem) {
                runCatching { gateway.authority.claim(AuthorityClaim(CompanionAuthorityScope.NowPlayingPlayback, SPOTIFY_APP_BUNDLE)) }
                runCatching { gateway.authority.claim(AuthorityClaim(CompanionAuthorityScope.NowPlayingMetadata, SPOTIFY_APP_BUNDLE)) }
                authorityHeld = true
            } else if (authorityHeld) {
                runCatching { gateway.authority.release(AuthorityRelease(CompanionAuthorityScope.NowPlayingPlayback)) }
                runCatching { gateway.authority.release(AuthorityRelease(CompanionAuthorityScope.NowPlayingMetadata)) }
                authorityHeld = false
            }
        }
    }

    private fun handleSocketDown() {
        nowPlayingObserver?.invoke(null)
        val gateway = gateway ?: return
        if (!authorityHeld) return
        authorityHeld = false
        scope.launch {
            runCatching { gateway.authority.release(AuthorityRelease(CompanionAuthorityScope.NowPlayingPlayback)) }
            runCatching { gateway.authority.release(AuthorityRelease(CompanionAuthorityScope.NowPlayingMetadata)) }
        }
    }

    override suspend fun handlePeerConnected() {
        val gateway = gateway ?: return
        authorityHeld = false
        val pending = lastSnapshotState ?: return
        if (pending.item == null) return
        runCatching { gateway.player.snapshot(buildSnapshot(pending)) }
        runCatching { gateway.authority.claim(AuthorityClaim(CompanionAuthorityScope.NowPlayingPlayback, SPOTIFY_APP_BUNDLE)) }
        runCatching { gateway.authority.claim(AuthorityClaim(CompanionAuthorityScope.NowPlayingMetadata, SPOTIFY_APP_BUNDLE)) }
        authorityHeld = true
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
        handleStateUpdate(newState)
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

        fun makeUpdate(state: PlayerState, heroEdge: Int, liked: Boolean?, likeSupported: Boolean?): NowPlayingUpdate {
            val media: MediaItemUpdate? = state.item?.let { item ->
                val title = item.name
                val artist = item.artists.joinToString(", ") { it.name }
                val album = (item as? PlayerItem.TrackItem)?.track?.album?.name
                val albumUri = (item as? PlayerItem.TrackItem)?.track?.album?.uri
                MediaItemUpdate(
                    persistentId = item.uri,
                    title = title.ifEmpty { null },
                    album = album,
                    albumUri = albumUri,
                    albumArtist = null,
                    artist = artist.ifEmpty { null },
                    artistUri = item.artists.firstOrNull()?.uri,
                    liked = liked,
                    artworkId = artworkId(item, heroEdge),
                    durationMs = maxOf(item.durationMs, 0).toUInt(),
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

        fun makeSnapshot(state: PlayerState, heroEdge: Int, liked: Boolean?, likeSupported: Boolean?): WirePlayerState {
            val track: MediaItem? = state.item?.let { item ->
                val title = item.name
                val artist = item.artists.joinToString(", ") { it.name }
                val album = (item as? PlayerItem.TrackItem)?.track?.album?.name
                val albumUri = (item as? PlayerItem.TrackItem)?.track?.album?.uri
                MediaItem(
                    uri = item.uri,
                    persistentId = item.uri,
                    title = title.ifEmpty { null },
                    album = album,
                    albumUri = albumUri,
                    albumArtist = null,
                    artist = artist.ifEmpty { null },
                    artistUri = item.artists.firstOrNull()?.uri,
                    liked = liked,
                    artworkId = artworkId(item, heroEdge),
                    durationMs = maxOf(item.durationMs, 0).toUInt(),
                    mediaTypes = null,
                    trackNumber = null,
                    trackCount = null,
                    isLikeSupported = likeSupported,
                    isBanSupported = null,
                    isBanned = null,
                    chapterCount = null,
                )
            }

            val allowSeek = state.actions?.disallows?.seeking?.let { !it } ?: true
            val playback = Playback(
                state = if (state.isPlaying) PlaybackState.Playing else PlaybackState.Paused,
                positionMs = maxOf(state.progressMs, 0).toUInt(),
                shuffle = state.shuffleState,
                shuffleMode = if (state.shuffleState) ShuffleMode.Songs else ShuffleMode.Off,
                repeat = mapRepeat(state.repeatState),
                queueIndex = null,
                queueCount = null,
                queueChapterIndex = null,
                setElapsedTimeAvailable = allowSeek,
                queueListAvail = null,
                appleMusicRadioAd = null,
            )

            val context = state.context?.let { PlaybackContext(uri = it.uri, name = null) }

            return WirePlayerState(
                track = track,
                playback = playback,
                queue = emptyList(),
                options = PlayerOptions(speed = 1.0f, crossfade_ms = null),
                context = context,
            )
        }

        fun artworkId(item: PlayerItem, maxEdge: Int): String? = imageAssetId(rawArtworkUrl(item, maxEdge) ?: "", maxEdge)

        fun rawArtworkUrl(item: PlayerItem, maxEdge: Int): String? {
            val url = bestImageUrl(item.imageUrl, maxEdge)
            return url.ifEmpty { null }
        }

        fun bestImageUrl(urls: SpotifyImageURLs, maxEdge: Int): String {
            if (maxEdge <= 64) {
                if (urls.small.isNotEmpty()) return urls.small
                if (urls.medium.isNotEmpty()) return urls.medium
                return urls.large
            }
            if (maxEdge <= 300) {
                if (urls.medium.isNotEmpty()) return urls.medium
                if (urls.large.isNotEmpty()) return urls.large
                return urls.small
            }
            if (urls.large.isNotEmpty()) return urls.large
            if (urls.medium.isNotEmpty()) return urls.medium
            return urls.small
        }

        fun imageAssetId(rawUrl: String, maxEdge: Int): String? {
            if (rawUrl.isEmpty()) return null
            if (rawUrl.startsWith(SCDN_IMAGE_PREFIX)) {
                return "$ASSET_ID_PREFIX$maxEdge/i${rawUrl.substring(SCDN_IMAGE_PREFIX.length)}"
            }
            val encoded = URLEncoder.encode(rawUrl, "UTF-8")
                .replace("+", "%20")
                .replace("*", "%2A")
                .replace("%7E", "~")
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
                    bmp,
                    (bmp.width * scale).toInt().coerceAtLeast(1),
                    (bmp.height * scale).toInt().coerceAtLeast(1),
                    true,
                )
            } else {
                bmp
            }
            val out = java.io.ByteArrayOutputStream()
            scaled.compress(android.graphics.Bitmap.CompressFormat.JPEG, 82, out)
            out.toByteArray()
        }.getOrNull()

        fun spotifyType(kind: ItemKind): String? = when (kind) {
            ItemKind.Track -> "track"
            ItemKind.Album -> "album"
            ItemKind.Artist -> "artist"
            ItemKind.Playlist -> "playlist"
            ItemKind.Show -> "show"
            ItemKind.PodcastEpisode -> "episode"
            ItemKind.Station -> null
        }

        fun mapRepeat(mode: RepeatMode): WireRepeat = when (mode) {
            RepeatMode.OFF -> WireRepeat.Off
            RepeatMode.TRACK -> WireRepeat.One
            RepeatMode.CONTEXT -> WireRepeat.All
        }
    }
}
