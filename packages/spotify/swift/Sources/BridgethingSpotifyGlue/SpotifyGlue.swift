import BridgethingGateway
import BridgethingGlue
import BridgethingLyrics
import BridgethingSchema
import Foundation
import Spotiny
#if canImport(FoundationNetworking)
    import FoundationNetworking
#endif
#if canImport(ImageIO)
    import CoreGraphics
    import ImageIO
#endif

public typealias WireRepeat = BridgethingSchema.RepeatMode
private typealias SpotinyRepeat = Spotiny.RepeatMode

private let assetIdPrefix = "spotify/img/"
private let scdnImagePrefix = "https://i.scdn.co/image/"
private let defaultHeroEdge = 248
private let defaultThumbEdge = 96
private let queueMax = 50
private let queueRunwayFloor = 8
private let spotifyAppBundle = "com.spotify.client"

public typealias SpotifyAuthenticatorFactory = @Sendable () -> any OAuthAuthenticator

public final class SpotifyGlue: BridgethingGlue, @unchecked Sendable {
    public static let name: String = "spotify"
    public static let displayName: String = "Spotify"

    public let capabilities: GlueCapabilities = [
        .streaming,
        .queue,
        .albumArt,
        .recommendations,
        .recentlyPlayed,
        .library,
        .playlists,
    ]

    public let uriSchemes: [String] = ["spotify"]
    public let musicProvider: MusicProvider = .spotify
    public let lyricsSupported: Bool = false

    public typealias TokenCallback = @Sendable (_ accessToken: String, _ refreshToken: String) -> Void

    private let authenticatorFactory: SpotifyAuthenticatorFactory
    private let initialAccessToken: String
    private let initialRefreshToken: String
    private let onTokensRefreshed: TokenCallback?
    private let urlSession: URLSession
    private let httpExecutor: (any SpotinyHTTPExecutor)?

    private var client: SpotinyClient?
    private var gateway: BridgethingGateway?
    private var heldScopes: Set<CompanionAuthorityScope> = []
    private var nowPlayingObserver: (@Sendable (GlueNowPlaying?) -> Void)?
    private var authObserver: (@Sendable (GlueAuthState) -> Void)?
    private var serviceHealthObserver: (@Sendable (GlueServiceHealth) -> Void)?
    private var connectTask: Task<Void, Never>?

    private var lastSentQueueOrder: [String] = []
    private var lastSentThumbEdge = defaultThumbEdge

    private var contextNames: [String: String] = [:]
    private var contextResolveInFlight: Set<String> = []
    private var lastSnapshotState: Spotiny.PlayerState?
    private var likedByUri: [String: Bool] = [:]
    private let contextLock = NSLock()

    private var artHeroEdge = defaultHeroEdge
    private var artThumbEdge = defaultThumbEdge
    private let artProfileLock = NSLock()

    public static let defaultImageSession: URLSession = {
        let cfg = URLSessionConfiguration.default
        cfg.timeoutIntervalForRequest = 6
        cfg.timeoutIntervalForResource = 15
        let artDir = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)
            .first?
            .appendingPathComponent("SpotifyArt", isDirectory: true)
        cfg.urlCache = URLCache(memoryCapacity: 8 << 20, diskCapacity: 200 << 20, directory: artDir)
        cfg.requestCachePolicy = .returnCacheDataElseLoad
        return URLSession(configuration: cfg)
    }()

    public init(
        authenticatorFactory: @escaping SpotifyAuthenticatorFactory,
        accessToken: String = "",
        refreshToken: String = "",
        onTokensRefreshed: TokenCallback? = nil,
        urlSession: URLSession = SpotifyGlue.defaultImageSession,
        httpExecutor: (any SpotinyHTTPExecutor)? = nil
    ) {
        self.authenticatorFactory = authenticatorFactory
        initialAccessToken = accessToken
        initialRefreshToken = refreshToken
        self.onTokensRefreshed = onTokensRefreshed
        self.urlSession = urlSession
        self.httpExecutor = httpExecutor
    }

    public func attach(gateway: BridgethingGateway) async throws {
        if self.gateway != nil { await detach() }

        self.gateway = gateway
        resetQueueDedup()

        guard !initialAccessToken.isEmpty || !initialRefreshToken.isEmpty else {
            // no tokens yet
            authObserver?(.pending(nil))
            return
        }
        authObserver?(.authenticated)

        let client = SpotinyClient(
            authenticator: authenticatorFactory(),
            delegate: self,
            accessToken: initialAccessToken,
            refreshToken: initialRefreshToken,
            httpExecutor: httpExecutor
        )
        self.client = client

        // Not awaited: auth lifecycle reaches the host via the spotiny delegate.
        connectTask = Task { [weak client] in
            await client?.connect()
        }
    }

    public func detach() async {
        // Stop emitting auth state while tearing down: cancellation races in
        // spotiny would otherwise fire authDidFail and emit a ghost `failed`.
        authObserver = nil
        serviceHealthObserver = nil

        connectTask?.cancel()
        connectTask = nil

        await releaseAllAuthority()

        nowPlayingObserver?(nil)
        nowPlayingObserver = nil

        resetQueueDedup()
        resetContextCache()
        client = nil
        gateway = nil
    }

    private func resetQueueDedup() {
        lastSentQueueOrder = []
    }

    private func resetContextCache() {
        contextLock.lock()
        contextNames.removeAll()
        contextResolveInFlight.removeAll()
        lastSnapshotState = nil
        likedByUri.removeAll()
        contextLock.unlock()
    }

    public func setNowPlayingObserver(_ observer: @escaping @Sendable (GlueNowPlaying?) -> Void) async {
        nowPlayingObserver = observer
    }

    public func setAuthObserver(_ observer: @escaping @Sendable (GlueAuthState) -> Void) async {
        authObserver = observer
    }

    public func setServiceHealthObserver(_ observer: @escaping @Sendable (GlueServiceHealth) -> Void) async {
        serviceHealthObserver = observer
        observer(.ok)
    }

    // MARK: - inbound dispatch

    public func play(_ uri: PlayUri) async throws {
        guard let client else { throw GlueError.detached }
        if let context = uri.context, let parsed = SpotifyURI(context.contextUri) {
            let skip = SpotifyURI(uri.uri)
            await client.player.play(uri: parsed, skipToUri: skip)
        } else if let parsed = SpotifyURI(uri.uri) {
            await client.player.play(uri: parsed)
        } else {
            throw GlueError.notImplemented
        }
    }

    public func queue(_ req: QueueUri) async throws {
        guard let client else { throw GlueError.detached }
        if case .index = req.position { throw GlueError.notImplemented }
        guard let parsed = SpotifyURI(req.uri) else { throw GlueError.notImplemented }
        await client.player.addItemToQueue(uri: parsed)
    }

    public func pause() async throws {
        guard let client else { throw GlueError.detached }
        await client.player.pause()
    }

    public func resume() async throws {
        guard let client else { throw GlueError.detached }
        await client.player.resume()
    }

    public func skipNext() async throws {
        guard let client else { throw GlueError.detached }
        await client.player.skipNext()
    }

    public func skipPrev() async throws {
        guard let client else { throw GlueError.detached }
        await client.player.skipPrevious()
    }

    public func seekTo(_ ms: UInt32) async throws {
        guard let client else { throw GlueError.detached }
        await client.player.seek(positionMs: Int(ms))
    }

    public func setShuffle(_ on: Bool) async throws {
        guard let client else { throw GlueError.detached }
        await client.player.setShuffle(on)
    }

    public func setRepeat(_ mode: WireRepeat) async throws {
        guard let client else { throw GlueError.detached }
        let mapped: SpotinyRepeat = switch mode {
        case .off: .off
        case .all: .context
        case .one: .track
        }
        await client.player.setRepeatMode(mapped)
    }

    // MARK: - library

    public func search(_ req: LibrarySearchRequest) async throws -> SearchResult {
        guard let client else { throw GlueError.detached }

        let kinds = (req.kinds?.isEmpty == false) ? req.kinds! : [.track, .album, .artist, .playlist]
        let types = kinds.compactMap(Self.spotifyType(for:))
        guard !types.isEmpty else {
            return SearchResult(items: [], kinds: [], total: nil, hasMore: false)
        }

        let results = await client.search.search(
            query: req.query, types: types, limit: Int(req.limit), offset: Int(req.offset)
        )

        let edge = artEdges().thumb
        let limit = Int(req.limit)
        var items: [LibraryItem] = []
        var presentKinds: [ItemKind] = []
        var reachedFullPage = false

        for kind in kinds {
            let kindItems: [LibraryItem]
            switch kind {
            case .track: kindItems = results.tracks.map { .track(Self.mapTrack($0, edge: edge)) }
            case .album: kindItems = results.albums.map { .album(Self.mapAlbum($0, edge: edge)) }
            case .artist: kindItems = results.artists.map { .artist(Self.mapArtist($0, edge: edge)) }
            case .playlist: kindItems = results.playlists.map { .playlist(Self.mapPlaylist($0, edge: edge)) }
            case .show: kindItems = results.shows.map { .show(Self.mapShow($0, edge: edge)) }
            case .podcastEpisode: kindItems = results.episodes.map { .podcastEpisode(Self.mapEpisode($0, edge: edge)) }
            case .station: kindItems = []
            }
            if !kindItems.isEmpty {
                presentKinds.append(kind)
                if kindItems.count >= limit { reachedFullPage = true }
            }
            items.append(contentsOf: kindItems)
        }

        return SearchResult(items: items, kinds: presentKinds, total: nil, hasMore: reachedFullPage)
    }

    public func browse(_ req: LibraryBrowseRequest) async throws -> BrowseResult {
        guard let client else { throw GlueError.detached }
        let limit = Int(req.limit)
        let offset = Int(req.offset)
        let edge = artEdges().thumb

        let result: BrowseResult
        switch req.nodeId {
        case nil, "", "root":
            result = await browseRoot(client)

        case Self.recentlyPlayedNode:
            // recently-played is cursor-based, not offset-paged.
            let entries = Self.dedupedTrackEntries(await client.player.getRecentlyPlayed(limit: 50), edge: edge)
            result = BrowseResult(entries: entries, total: UInt32(entries.count), hasMore: false)

        case Self.topTracksNode:
            let page = await client.tracks.getUserTopTracks(limit: limit, offset: offset)
            let entries = page.items.map { BrowseEntry.item(.track(Self.mapTrack($0, edge: edge))) }
            result = Self.section(entries, pageCount: page.items.count, total: page.total, offset: offset)

        case Self.homeNode:
            let page = await client.categories.getMadeForYou(limit: limit, offset: offset)
            let entries = page.items.map { BrowseEntry.item(.playlist(Self.mapPlaylist($0, edge: edge))) }
            result = Self.section(entries, pageCount: page.items.count, total: page.total, offset: offset)

        case Self.playlistsNode:
            let page = await client.playlists.getUserPlaylists(limit: limit, offset: offset)
            var entries: [BrowseEntry] = []
            if offset == 0 {
                let userId = await client.users.getCurrentUser()?.id
                if let liked = Self.likedSongsEntry(userId: userId) { entries.append(liked) }
            }
            entries += page.items.map { BrowseEntry.item(.playlist(Self.mapPlaylist($0, edge: edge))) }
            result = Self.section(entries, pageCount: page.items.count, total: page.total, offset: offset)

        case Self.podcastsNode:
            let page = await client.shows.getUserSavedShows(limit: limit, offset: offset)
            var entries: [BrowseEntry] = []
            if offset == 0 { entries.append(Self.yourEpisodesEntry()) }
            entries += page.items.map { BrowseEntry.item(.show(Self.mapShow($0, edge: edge))) }
            result = Self.section(entries, pageCount: page.items.count, total: page.total, offset: offset)

        case Self.artistsNode:
            let page = await client.artists.getUserFollowedArtists(limit: limit, offset: offset)
            let entries = page.items.map { BrowseEntry.item(.artist(Self.mapArtist($0, edge: edge))) }
            result = Self.section(entries, pageCount: page.items.count, total: page.total, offset: offset)

        case Self.albumsNode:
            let page = await client.albums.getUserSavedAlbums(limit: limit, offset: offset)
            let entries = page.items.map { BrowseEntry.item(.album(Self.mapAlbum($0, edge: edge))) }
            result = Self.section(entries, pageCount: page.items.count, total: page.total, offset: offset)

        default:
            guard let uri = SpotifyURI(req.nodeId ?? "") else {
                return BrowseResult(entries: [], total: nil, hasMore: false)
            }
            result = await browseChildren(client, uri, limit: limit, offset: offset)
        }

        warmArt(in: result)
        return result
    }

    /// Drill-in: children of an individual library item (playlist/album/artist/show + the liked/your-episodes pseudo nodes).
    private func browseChildren(_ client: SpotinyClient, _ uri: SpotifyURI, limit: Int, offset: Int) async -> BrowseResult {
        let edge = artEdges().thumb
        switch uri.kind {
        case .playlist, .playlistV2:
            let page = await client.playlists.getPlaylistItems(uri: uri, limit: limit, offset: offset)
            let entries = page.items.map { Self.mapPlaylistItem($0, edge: edge) }
            return Self.section(entries, pageCount: page.items.count, total: page.total, offset: offset)

        case .album:
            let page = await client.albums.getAlbumTracks(uri: uri, limit: limit, offset: offset)
            let entries = page.items.map { BrowseEntry.item(.track(Self.mapTrack($0, edge: edge))) }
            return Self.section(entries, pageCount: page.items.count, total: page.total, offset: offset)

        case .artist, .artistToplist:
            // artist top-tracks is not offset-paged.
            let entries = (await client.artists.getArtistTopTracks(uri: uri)).map { BrowseEntry.item(.track(Self.mapTrack($0, edge: edge))) }
            return BrowseResult(entries: entries, total: UInt32(entries.count), hasMore: false)

        case .show:
            let page = await client.shows.getShowEpisodes(uri: uri, limit: limit, offset: offset)
            let entries = page.items.map { BrowseEntry.item(.podcastEpisode(Self.mapEpisode($0, edge: edge))) }
            return Self.section(entries, pageCount: page.items.count, total: page.total, offset: offset)

        case .collection:
            let page = await client.tracks.getUserSavedTracks(limit: limit, offset: offset)
            let entries = page.items.map { BrowseEntry.item(.track(Self.mapTrack($0, edge: edge, saved: true))) }
            return Self.section(entries, pageCount: page.items.count, total: page.total, offset: offset)

        case ._yourEpisodes:
            let page = await client.episodes.getUserSavedEpisodes(limit: limit, offset: offset)
            let entries = page.items.map { BrowseEntry.item(.podcastEpisode(Self.mapEpisode($0, edge: edge))) }
            return Self.section(entries, pageCount: page.items.count, total: page.total, offset: offset)

        default:
            return BrowseResult(entries: [], total: nil, hasMore: false)
        }
    }

    /// Each section folder inlines a preview slice of its children.
    private func browseRoot(_ client: SpotinyClient) async -> BrowseResult {
        let previewLimit = 14
        let edge = artEdges().thumb

        async let recentP = client.player.getRecentlyPlayed(limit: previewLimit)
        async let topP = client.tracks.getUserTopTracks(limit: previewLimit)
        async let homeP = client.categories.getMadeForYou(limit: 10)
        async let playlistsP = client.playlists.getUserPlaylists(limit: previewLimit)
        async let showsP = client.shows.getUserSavedShows(limit: previewLimit)
        async let artistsP = client.artists.getUserFollowedArtists(limit: previewLimit)
        async let albumsP = client.albums.getUserSavedAlbums(limit: previewLimit)
        async let userP = client.users.getCurrentUser()

        let recentChildren = Self.dedupedTrackEntries(await recentP, edge: edge)
        let topChildren = (await topP).items.map { BrowseEntry.item(.track(Self.mapTrack($0, edge: edge))) }
        let homeChildren = (await homeP).items.map { BrowseEntry.item(.playlist(Self.mapPlaylist($0, edge: edge))) }

        var playlistChildren: [BrowseEntry] = []
        if let liked = Self.likedSongsEntry(userId: (await userP)?.id) { playlistChildren.append(liked) }
        playlistChildren += (await playlistsP).items.map { BrowseEntry.item(.playlist(Self.mapPlaylist($0, edge: edge))) }

        var podcastChildren: [BrowseEntry] = [Self.yourEpisodesEntry()]
        podcastChildren += (await showsP).items.map { BrowseEntry.item(.show(Self.mapShow($0, edge: edge))) }

        let artistChildren = (await artistsP).items.map { BrowseEntry.item(.artist(Self.mapArtist($0, edge: edge))) }
        let albumChildren = (await albumsP).items.map { BrowseEntry.item(.album(Self.mapAlbum($0, edge: edge))) }

        var folders: [BrowseEntry] = []
        func addSection(_ nodeId: String, _ title: String, _ children: [BrowseEntry], total: UInt32?) {
            guard !children.isEmpty else { return }
            folders.append(.folder(BrowseFolder(
                nodeId: nodeId, title: title, subtitle: nil, artworkId: nil,
                total: total, previewChildren: children
            )))
        }
        addSection(Self.recentlyPlayedNode, "Recently Played", recentChildren, total: UInt32(recentChildren.count))
        addSection(Self.topTracksNode, "Top Tracks", topChildren, total: UInt32(topChildren.count))
        addSection(Self.homeNode, "Home", homeChildren, total: UInt32(homeChildren.count))
        addSection(Self.playlistsNode, "Playlists", playlistChildren, total: nil)
        addSection(Self.podcastsNode, "Podcasts", podcastChildren, total: nil)
        addSection(Self.artistsNode, "Artists", artistChildren, total: nil)
        addSection(Self.albumsNode, "Albums", albumChildren, total: nil)
        let result = BrowseResult(entries: folders, total: UInt32(folders.count), hasMore: false)
        return result
    }

    private static func section(_ entries: [BrowseEntry], pageCount: Int, total: Int, offset: Int) -> BrowseResult {
        BrowseResult(entries: entries, total: UInt32(max(total, 0)), hasMore: offset + pageCount < total)
    }

    // recently-played repeats tracks; dedupe by uri.
    private static func dedupedTrackEntries(_ tracks: [Spotiny.Track], edge: Int) -> [BrowseEntry] {
        var seen = Set<String>()
        return tracks.compactMap { track in
            guard seen.insert(track.uri).inserted else { return nil }
            return .item(.track(Self.mapTrack(track, edge: edge)))
        }
    }

    private static func likedSongsEntry(userId: String?) -> BrowseEntry? {
        guard let userId, let uri = SpotifyURI(kind: .collection, id: userId) else { return nil }
        return .item(.playlist(BridgethingSchema.Playlist(
            uri: uri.string(), name: "Liked Songs", ownerName: nil, trackCount: nil, artworkId: nil
        )))
    }

    private static func yourEpisodesEntry() -> BrowseEntry {
        .item(.playlist(BridgethingSchema.Playlist(
            uri: SpotifyURI.Static.yourEpisodes, name: "Your Episodes",
            ownerName: nil, trackCount: nil, artworkId: nil
        )))
    }

    public func resolveContext(_ uri: String) async throws -> ContextResolveReply {
        guard let client else { throw GlueError.detached }
        guard let parsed = SpotifyURI(uri) else { throw GlueError.notImplemented }
        let heroEdge = artEdges().hero
        func reply(_ name: String?, _ imageUrl: Spotiny.SpotifyImageURLs?, subtitle: String? = nil) -> ContextResolveReply {
            let artworkId = imageUrl.flatMap { Self.imageAssetId(Self.bestImageURL($0, maxEdge: heroEdge), maxEdge: heroEdge) }
            return ContextResolveReply(name: name, artworkId: artworkId, subtitle: subtitle)
        }
        switch parsed.kind {
        case .playlist, .playlistV2:
            let p = await client.playlists.getPlaylist(uri: parsed)
            return reply(p?.name, p?.imageUrl)
        case .album:
            let a = await client.albums.getAlbum(uri: parsed)
            return reply(a?.name, a?.imageUrl, subtitle: a?.artists.first?.name)
        case .show:
            let s = await client.shows.getShow(uri: parsed)
            return reply(s?.name, s?.imageUrl)
        case .artist, .artistToplist:
            let ar = await client.artists.getArtist(uri: parsed)
            return reply(ar?.name, ar?.imageUrl)
        default:
            throw GlueError.notImplemented
        }
    }

    public func recommendations(_ req: LibraryRecommendationsRequest) async throws -> RecommendationsResult {
        guard let client else { throw GlueError.detached }
        let limit = Int(req.limit)

        let edge = artEdges().thumb
        let seedTracks = req.seeds.filter { $0.kind == .track }.compactMap { SpotifyURI($0.uri)?.id }
        let seedArtists = req.seeds.filter { $0.kind == .artist }.compactMap { SpotifyURI($0.uri)?.id }

        let tracks = await client.recommendations.get(seedTracks: seedTracks, seedArtists: seedArtists, limit: limit)
        if !tracks.isEmpty {
            return RecommendationsResult(items: tracks.map { .track(Self.mapTrack($0, edge: edge)) }, total: nil, hasMore: false)
        }

        if let artistSeed = req.seeds.first(where: { $0.kind == .artist }), let uri = SpotifyURI(artistSeed.uri) {
            let top = Array((await client.artists.getArtistTopTracks(uri: uri)).prefix(limit))
            return RecommendationsResult(items: top.map { .track(Self.mapTrack($0, edge: edge)) }, total: nil, hasMore: false)
        }

        return RecommendationsResult(items: [], total: nil, hasMore: false)
    }

    public func favoritesList(_ req: LibraryFavoritesListRequest) async throws -> FavoritesPage {
        guard let client else { throw GlueError.detached }
        let offset = Int(req.offset)
        let edge = artEdges().thumb
        let page = await client.tracks.getUserSavedTracks(limit: Int(req.limit), offset: offset)
        let items = page.items.map { LibraryItem.track(Self.mapTrack($0, edge: edge, saved: true)) }
        return FavoritesPage(items: items, total: UInt32(max(page.total, 0)), hasMore: offset + page.items.count < page.total)
    }

    public func favoritesContains(_ req: LibraryFavoritesContainsRequest) async throws -> [Bool] {
        guard let client else { throw GlueError.detached }
        // iap2 now-playing uris parse as valid kinds but carry non-base62 ids; the empty
        // sentinel keeps index alignment while skipping the spotify lookup for them.
        let uris = req.uris.map { Self.spotifyURI($0) != nil ? $0 : "" }
        return await client.library.contains(uris: uris)
    }

    public func favoritesToggle(_ item: ItemRef) async throws {
        guard let client else { throw GlueError.detached }
        guard let uri = Self.spotifyURI(item.uri) else { throw GlueError.notImplemented }
        let saved = (await client.library.contains(uris: [item.uri])).first ?? false
        if saved {
            await client.library.remove(uris: [uri])
        } else {
            await client.library.save(uris: [uri])
        }
        await applyLikedChange(uri: item.uri, liked: !saved)
    }

    public func favoritesSet(_ item: ItemRef, liked: Bool) async throws {
        guard let client else { throw GlueError.detached }
        guard let uri = Self.spotifyURI(item.uri) else { throw GlueError.notImplemented }
        if liked {
            await client.library.save(uris: [uri])
        } else {
            await client.library.remove(uris: [uri])
        }
        await applyLikedChange(uri: item.uri, liked: liked)
    }

    public func favoritesSetMany(_ entries: [FavoritesSet]) async throws {
        guard let client else { throw GlueError.detached }
        let toSave = entries.filter { $0.liked }.compactMap { Self.spotifyURI($0.item.uri) }
        let toRemove = entries.filter { !$0.liked }.compactMap { Self.spotifyURI($0.item.uri) }
        if !toSave.isEmpty { await client.library.save(uris: toSave) }
        if !toRemove.isEmpty { await client.library.remove(uris: toRemove) }
        for entry in entries { await applyLikedChange(uri: entry.item.uri, liked: entry.liked) }
    }

    private func applyLikedChange(uri: String, liked: Bool) async {
        cacheLiked(liked, forUri: uri)
        await reemitSnapshotIfCurrent(uri: uri)
    }

    private static func spotifyURI(_ raw: String) -> SpotifyURI? {
        guard let uri = SpotifyURI(raw), uri.namespace == "spotify" else { return nil }
        return uri
    }

    public func debugState() async -> GlueDebugState {
        GlueDebugState(
            authorityPlaybackHeld: heldScopes.contains(.nowPlayingPlayback),
            authorityMetadataHeld: heldScopes.contains(.nowPlayingMetadata)
        )
    }

    private func warmArt(in result: BrowseResult) {
        for id in Set(Self.collectArtIds(result.entries)) {
            guard let parsed = Self.parseImageId(id) else { continue }
            Task { [urlSession] in _ = try? await urlSession.data(from: parsed.url) }
        }
    }

    private static func collectArtIds(_ entries: [BrowseEntry]) -> [String] {
        entries.flatMap { entry -> [String] in
            switch entry {
            case let .folder(folder):
                return (folder.artworkId.map { [$0] } ?? []) + collectArtIds(folder.previewChildren ?? [])
            case let .item(item):
                return item.artworkId.map { [$0] } ?? []
            }
        }
    }

    public func asset(id: String) async throws -> AssetBytes? {
        guard let parsed = Self.parseImageId(id) else { return nil }
        let (data, response) = try await urlSession.data(from: parsed.url)
        return autoreleasepool {
            if let scaled = Self.downsample(data, maxEdge: parsed.maxEdge) {
                return AssetBytes(bytes: scaled, mime: "image/jpeg")
            }
            let mime = (response as? HTTPURLResponse)?.value(forHTTPHeaderField: "Content-Type")
            return AssetBytes(bytes: data, mime: mime)
        }
    }

    // MARK: - outbound

    fileprivate func handleStateUpdate(_ state: Spotiny.PlayerState, reason: String) {
        guard let gateway else { return }
        let heroEdge = artEdges().hero
        let currentUri = state.item?.uri
        let (liked, likeSupported) = likeFields(forUri: currentUri)
        let update = Self.makeUpdate(from: state, heroEdge: heroEdge, liked: liked, likeSupported: likeSupported)
        let artworkUrl = state.item.flatMap { Self.rawArtworkURL(for: $0, maxEdge: heroEdge) }
        nowPlayingObserver?(GlueNowPlaying(update: update, artworkUrl: artworkUrl))

        let hasItem = state.item != nil
        contextLock.withLock { lastSnapshotState = state }
        let snapshot = makeSnapshot(from: state, heroEdge: heroEdge, liked: liked, likeSupported: likeSupported)
        let thumbEdge = artEdges().thumb

        if let uri = currentUri, Self.spotifyURI(uri) != nil, cachedLiked(forUri: uri) == nil {
            Task { [weak self] in await self?.resolveLiked(forUri: uri) }
        }

        Task { [weak self] in
            guard let self else { return }
            if hasItem {
                await self.claimAuthority([.nowPlayingPlayback, .nowPlayingMetadata])
            } else if !self.heldScopes.isEmpty {
                await self.releaseAllAuthority()
            }
            try? await gateway.player.snapshot(snapshot)
            await self.sendQueue(thumbEdge: thumbEdge)
        }
    }

    private func makeSnapshot(from state: Spotiny.PlayerState, heroEdge: Int, liked: Bool?, likeSupported: Bool?) -> BridgethingSchema.PlayerState {
        let track: MediaItem? = state.item.map { item in
            let title = item.name
            let artist = item.artists.map(\.name).joined(separator: ", ")
            let album: String? = if case let .track(t) = item { t.album?.name } else { nil }
            let albumUri: String? = if case let .track(t) = item { t.album?.uri } else { nil }
            return MediaItem(
                uri: item.uri,
                persistentId: item.uri,
                title: title.isEmpty ? nil : title,
                album: album,
                albumUri: albumUri,
                albumArtist: nil,
                artist: artist.isEmpty ? nil : artist,
                artistUri: item.artists.first?.uri,
                liked: liked,
                artworkId: Self.artworkId(for: item, maxEdge: heroEdge),
                durationMs: UInt32(max(item.duration_ms, 0)),
                mediaTypes: nil,
                trackNumber: nil,
                trackCount: nil,
                isLikeSupported: likeSupported,
                isBanSupported: nil,
                isBanned: nil,
                chapterCount: nil
            )
        }
        let allowSeek = state.actions?.disallows?.seeking.map { !$0 } ?? true
        let playback = Playback(
            state: state.is_playing ? .playing : .paused,
            positionMs: UInt32(max(state.progress_ms, 0)),
            shuffle: state.shuffle_state,
            shuffleMode: state.shuffle_state ? .songs : .off,
            repeat: Self.mapRepeat(state.repeat_state),
            queueIndex: nil,
            queueCount: nil,
            queueChapterIndex: nil,
            setElapsedTimeAvailable: allowSeek,
            queueListAvail: nil,
            appleMusicRadioAd: nil
        )
        let context: PlaybackContext? = state.context.map {
            PlaybackContext(uri: $0.uri, name: contextName(for: $0.uri))
        }
        return BridgethingSchema.PlayerState(
            track: track,
            playback: playback,
            queue: [],
            options: PlayerOptions(speed: 1.0, crossfade_ms: nil),
            context: context
        )
    }

    private func sendQueue(thumbEdge: Int) async {
        guard let client else { return }
        let queueItems = Array((await client.player.getQueue())?.queue.prefix(queueMax) ?? [])
        let entries = queueItems.map { Self.queueItem(from: $0, maxEdge: thumbEdge) }
        await sendQueueChangedIfNeeded(entries, thumbEdge: thumbEdge)
    }

    private func contextName(for uri: String) -> String? {
        contextLock.lock()
        if let cached = contextNames[uri] {
            contextLock.unlock()
            return cached
        }
        let shouldResolve = !uri.isEmpty && !contextResolveInFlight.contains(uri)
        if shouldResolve { contextResolveInFlight.insert(uri) }
        contextLock.unlock()
        if shouldResolve {
            Task { [weak self] in await self?.resolveContextName(uri) }
        }
        return nil
    }

    private func resolveContextName(_ uri: String) async {
        let resolved = try? await resolveContext(uri)
        let name = resolved?.name.flatMap { $0.isEmpty ? nil : $0 }
        let pending = contextLock.withLock { () -> Spotiny.PlayerState? in
            contextResolveInFlight.remove(uri)
            if let name { contextNames[uri] = name }
            return lastSnapshotState
        }
        guard name != nil, let pending, pending.context?.uri == uri, let gateway else { return }
        try? await gateway.player.snapshot(buildSnapshot(from: pending))
    }

    private func likeFields(forUri uri: String?) -> (liked: Bool?, supported: Bool?) {
        guard let uri, Self.spotifyURI(uri) != nil else { return (nil, nil) }
        return (cachedLiked(forUri: uri), true)
    }

    private func cachedLiked(forUri uri: String) -> Bool? {
        contextLock.lock()
        defer { contextLock.unlock() }
        return likedByUri[uri]
    }

    private func cacheLiked(_ liked: Bool, forUri uri: String) {
        contextLock.lock()
        likedByUri[uri] = liked
        contextLock.unlock()
    }

    private func buildSnapshot(from state: Spotiny.PlayerState) -> BridgethingSchema.PlayerState {
        let (liked, supported) = likeFields(forUri: state.item?.uri)
        return makeSnapshot(from: state, heroEdge: artEdges().hero, liked: liked, likeSupported: supported)
    }

    private func resolveLiked(forUri uri: String) async {
        guard let client else { return }
        let liked = (await client.library.contains(uris: [uri])).first ?? false
        cacheLiked(liked, forUri: uri)
        await reemitSnapshotIfCurrent(uri: uri)
    }

    private func reemitSnapshotIfCurrent(uri: String) async {
        let pending = contextLock.withLock { lastSnapshotState }
        guard let pending, pending.item?.uri == uri, let gateway else { return }
        try? await gateway.player.snapshot(buildSnapshot(from: pending))
    }

    private func sendQueueChangedIfNeeded(_ entries: [QueueItem], thumbEdge: Int) async {
        guard let gateway else { return }
        let order = entries.map(\.uri)

        let edgeChanged = thumbEdge != lastSentThumbEdge
        lastSentThumbEdge = thumbEdge
        if !edgeChanged,
           let runway = forwardSlideRunway(from: lastSentQueueOrder, to: order),
           runway >= queueRunwayFloor {
            return
        }

        do {
            try await gateway.player.queueChanged(QueueSnapshot(order: order, items: entries))
            lastSentQueueOrder = order
        } catch {
            // leave last-sent state unchanged so the next change re-sends.
        }
    }

    private func forwardSlideRunway(from last: [String], to new: [String]) -> Int? {
        guard !last.isEmpty else { return nil }
        for k in 0..<last.count {
            let suffix = Array(last[k...])
            if new.count >= suffix.count && Array(new.prefix(suffix.count)) == suffix {
                return suffix.count
            }
        }
        return nil
    }

    public func setArtProfile(heroPx: Int, thumbPx: Int) {
        artProfileLock.lock()
        defer { artProfileLock.unlock() }
        artHeroEdge = max(1, heroPx)
        artThumbEdge = max(1, thumbPx)
    }

    private func artEdges() -> (hero: Int, thumb: Int) {
        artProfileLock.lock()
        defer { artProfileLock.unlock() }
        return (artHeroEdge, artThumbEdge)
    }

    private func claimAuthority(_ scopes: Set<CompanionAuthorityScope>) async {
        guard let gateway else { return }
        for scope in scopes where !heldScopes.contains(scope) {
            try? await gateway.authority.claim(AuthorityClaim(scope: scope, appBundle: spotifyAppBundle))
            heldScopes.insert(scope)
        }
    }

    private func releaseAllAuthority() async {
        guard let gateway else { return }
        for scope in heldScopes {
            try? await gateway.authority.release(AuthorityRelease(scope: scope))
        }
        heldScopes.removeAll()
    }

    fileprivate func handleSocketDown() {
        nowPlayingObserver?(nil)
        guard gateway != nil, !heldScopes.isEmpty else { return }
        Task { await releaseAllAuthority() }
    }

    public func handlePeerConnected() async {
        guard let gateway else { return }
        heldScopes.removeAll()
        resetQueueDedup()
        let pending = contextLock.withLock { lastSnapshotState }
        guard let pending, pending.item != nil else { return }
        await claimAuthority([.nowPlayingPlayback, .nowPlayingMetadata])
        try? await gateway.player.snapshot(buildSnapshot(from: pending))
        await sendQueue(thumbEdge: artEdges().thumb)
    }

    private static func makeUpdate(from state: Spotiny.PlayerState, heroEdge: Int, liked: Bool?, likeSupported: Bool?) -> NowPlayingUpdate {
        let media: MediaItemUpdate? = state.item.map { item in
            let title = item.name
            let artist = item.artists.map(\.name).joined(separator: ", ")
            let album: String? = if case let .track(track) = item { track.album?.name } else { nil }
            let albumUri: String? = if case let .track(track) = item { track.album?.uri } else { nil }
            return MediaItemUpdate(
                persistentId: item.uri,
                title: title.isEmpty ? nil : title,
                album: album,
                albumUri: albumUri,
                albumArtist: nil,
                artist: artist.isEmpty ? nil : artist,
                artistUri: item.artists.first?.uri,
                liked: liked,
                artworkId: artworkId(for: item, maxEdge: heroEdge),
                durationMs: UInt32(max(item.duration_ms, 0)),
                mediaTypes: nil,
                trackNumber: nil,
                trackCount: nil,
                isLikeSupported: likeSupported,
                isBanSupported: nil,
                isBanned: nil,
                isResidentOnDevice: nil,
                chapterCount: nil
            )
        }

        let allowSeek = state.actions?.disallows?.seeking.map { !$0 } ?? true
        let playback = PlaybackUpdate(
            playing: state.is_playing,
            positionMs: UInt32(max(state.progress_ms, 0)),
            shuffle: state.shuffle_state,
            shuffleMode: state.shuffle_state ? .songs : .off,
            repeat: mapRepeat(state.repeat_state),
            appBundle: "com.spotify.client",
            appDisplayName: "Spotify",
            queueIndex: nil,
            queueCount: nil,
            queueChapterIndex: nil,
            playbackSpeed: nil,
            setElapsedTimeAvailable: allowSeek,
            queueListAvail: nil,
            appleMusicRadioAd: nil,
            appleMusicRadioStationName: nil
        )

        return NowPlayingUpdate(mediaItem: media, playback: playback)
    }

    private static func artworkId(for item: PlayerItem, maxEdge: Int) -> String? {
        imageAssetId(rawArtworkURL(for: item, maxEdge: maxEdge) ?? "", maxEdge: maxEdge)
    }

    private static func queueItem(from item: PlayerItem, maxEdge: Int) -> QueueItem {
        let artist = item.artists.map(\.name).joined(separator: ", ")
        let album: String? = if case let .track(track) = item { track.album?.name } else { nil }
        let albumUri: String? = if case let .track(track) = item { track.album?.uri } else { nil }
        return QueueItem(
            uri: item.uri,
            title: item.name.isEmpty ? nil : item.name,
            artist: artist.isEmpty ? nil : artist,
            artistUri: item.artists.first?.uri,
            album: album,
            albumUri: albumUri,
            artworkId: artworkId(for: item, maxEdge: maxEdge),
            durationMs: UInt32(max(item.duration_ms, 0)),
            persistentId: nil
        )
    }

    fileprivate static func rawArtworkURL(for item: PlayerItem, maxEdge: Int) -> String? {
        let url = bestImageURL(item.imageUrl, maxEdge: maxEdge)
        return url.isEmpty ? nil : url
    }

    // MARK: - mapping

    private static let recentlyPlayedNode = "recently-played"
    private static let topTracksNode = "top-tracks"
    private static let homeNode = "home"
    private static let playlistsNode = "playlists"
    private static let podcastsNode = "podcasts"
    private static let artistsNode = "artists"
    private static let albumsNode = "albums"

    private static func bestImageURL(_ urls: SpotifyImageURLs, maxEdge: Int) -> String {
        if maxEdge <= 64 {
            if !urls.small.isEmpty { return urls.small }
            if !urls.medium.isEmpty { return urls.medium }
            return urls.large
        }
        if maxEdge <= 300 {
            if !urls.medium.isEmpty { return urls.medium }
            if !urls.large.isEmpty { return urls.large }
            return urls.small
        }
        if !urls.large.isEmpty { return urls.large }
        if !urls.medium.isEmpty { return urls.medium }
        return urls.small
    }

    private static func imageAssetId(_ rawURL: String, maxEdge: Int) -> String? {
        guard !rawURL.isEmpty else { return nil }
        if rawURL.hasPrefix(scdnImagePrefix) {
            return "\(assetIdPrefix)\(maxEdge)/i\(rawURL.dropFirst(scdnImagePrefix.count))"
        }
        guard let encoded = rawURL.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) else { return nil }
        return "\(assetIdPrefix)\(maxEdge)/u\(encoded)"
    }

    private static func parseImageId(_ id: String) -> (url: URL, maxEdge: Int)? {
        guard id.hasPrefix(assetIdPrefix) else { return nil }
        let rest = id.dropFirst(assetIdPrefix.count)
        guard let slash = rest.firstIndex(of: "/"),
              let maxEdge = Int(rest[..<slash])
        else { return nil }
        let tagged = rest[rest.index(after: slash)...]
        guard let tag = tagged.first else { return nil }
        let body = String(tagged.dropFirst())
        let urlString: String
        switch tag {
        case "i": urlString = scdnImagePrefix + body
        case "u": guard let decoded = body.removingPercentEncoding else { return nil }; urlString = decoded
        default: return nil
        }
        guard let url = URL(string: urlString) else { return nil }
        return (url, maxEdge)
    }

    private static func downsample(_ data: Data, maxEdge: Int) -> Data? {
        #if canImport(ImageIO)
            guard let src = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
            let opts: [CFString: Any] = [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceThumbnailMaxPixelSize: maxEdge,
            ]
            guard let thumb = CGImageSourceCreateThumbnailAtIndex(src, 0, opts as CFDictionary) else { return nil }
            let out = NSMutableData()
            guard let dest = CGImageDestinationCreateWithData(out as CFMutableData, "public.jpeg" as CFString, 1, nil)
            else { return nil }
            CGImageDestinationAddImage(dest, thumb, [kCGImageDestinationLossyCompressionQuality: 0.82] as CFDictionary)
            guard CGImageDestinationFinalize(dest) else { return nil }
            return out as Data
        #else
            return nil
        #endif
    }

    private static func spotifyType(for kind: ItemKind) -> String? {
        switch kind {
        case .track: "track"
        case .album: "album"
        case .artist: "artist"
        case .playlist: "playlist"
        case .show: "show"
        case .podcastEpisode: "episode"
        case .station: nil
        }
    }

    private static func mapTrack(_ t: Spotiny.Track, edge: Int, saved: Bool = false) -> BridgethingSchema.Track {
        let primary = t.artists.first
        return BridgethingSchema.Track(
            id: t.uri,
            name: t.name,
            album: BridgethingSchema.Album(id: t.album?.uri ?? "", name: t.album?.name ?? "", artwork_id: nil),
            artist: BridgethingSchema.Artist(id: primary?.uri ?? "", name: primary?.name ?? "", artwork_id: nil),
            artists: t.artists.map { BridgethingSchema.Artist(id: $0.uri, name: $0.name, artwork_id: nil) },
            duration_ms: UInt32(max(t.duration_ms, 0)),
            image_id: imageAssetId(bestImageURL(t.imageUrl, maxEdge: edge), maxEdge: edge) ?? "",
            saved: saved
        )
    }

    private static func mapPlaylistItem(_ item: Spotiny.PlaylistItem, edge: Int) -> BrowseEntry {
        if item.type == "episode" {
            return .item(.podcastEpisode(BridgethingSchema.PodcastEpisode(
                uri: item.uri,
                name: item.name ?? "",
                showName: nil,
                durationMs: UInt32(max(item.duration_ms, 0)),
                publishedAtUnixS: nil,
                artworkId: imageAssetId(bestImageURL(Spotiny.SpotifyImageURLs(item.images), maxEdge: edge), maxEdge: edge)
            )))
        }
        let primary = item.artists.first
        return .item(.track(BridgethingSchema.Track(
            id: item.uri,
            name: item.name ?? "",
            album: BridgethingSchema.Album(id: item.album?.uri ?? "", name: item.album?.name ?? "", artwork_id: nil),
            artist: BridgethingSchema.Artist(id: primary?.uri ?? "", name: primary?.name ?? "", artwork_id: nil),
            artists: item.artists.map { BridgethingSchema.Artist(id: $0.uri, name: $0.name, artwork_id: nil) },
            duration_ms: UInt32(max(item.duration_ms, 0)),
            image_id: imageAssetId(bestImageURL(item.imageUrl, maxEdge: edge), maxEdge: edge) ?? "",
            saved: false
        )))
    }

    private static func mapAlbum(_ a: Spotiny.Album, edge: Int) -> BridgethingSchema.Album {
        BridgethingSchema.Album(
            id: a.uri,
            name: a.name,
            artwork_id: imageAssetId(bestImageURL(a.imageUrl, maxEdge: edge), maxEdge: edge)
        )
    }

    private static func mapArtist(_ a: Spotiny.Artist, edge: Int) -> BridgethingSchema.Artist {
        BridgethingSchema.Artist(
            id: a.uri,
            name: a.name,
            artwork_id: imageAssetId(bestImageURL(a.imageUrl, maxEdge: edge), maxEdge: edge)
        )
    }

    private static func mapPlaylist(_ p: Spotiny.Playlist, edge: Int) -> BridgethingSchema.Playlist {
        BridgethingSchema.Playlist(
            uri: p.uri,
            name: p.name,
            ownerName: nil,
            trackCount: nil,
            artworkId: imageAssetId(bestImageURL(p.imageUrl, maxEdge: edge), maxEdge: edge)
        )
    }

    private static func mapShow(_ s: Spotiny.Show, edge: Int) -> BridgethingSchema.Show {
        BridgethingSchema.Show(
            uri: s.uri,
            name: s.name,
            publisher: nil,
            episodeCount: nil,
            artworkId: imageAssetId(bestImageURL(s.imageUrl, maxEdge: edge), maxEdge: edge)
        )
    }

    private static func mapEpisode(_ e: Spotiny.Episode, edge: Int) -> BridgethingSchema.PodcastEpisode {
        BridgethingSchema.PodcastEpisode(
            uri: e.uri,
            name: e.name,
            showName: e.show?.name,
            durationMs: UInt32(max(e.duration_ms, 0)),
            publishedAtUnixS: nil,
            artworkId: imageAssetId(bestImageURL(e.imageUrl, maxEdge: edge), maxEdge: edge)
        )
    }

    private static func mapRepeat(_ mode: SpotinyRepeat) -> WireRepeat {
        switch mode {
        case .off: .off
        case .track: .one
        case .context: .all
        }
    }
}

extension SpotifyGlue: SpotinyDelegate {
    public func authDidRefresh(accessToken: String, refreshToken: String) {
        onTokensRefreshed?(accessToken, refreshToken)
        // Empty tokens mean spotiny cleared state on a failed attempt (authDidFail
        // follows); don't emit `authenticated` without credentials.
        if !accessToken.isEmpty {
            authObserver?(.authenticated)
        }
    }

    public func authDidFail(reason: String) {
        handleSocketDown()
        authObserver?(.failed(reason))
    }

    public func socketDidConnect() {}

    public func socketDidDisconnect() {
        handleSocketDown()
    }

    public func playerStateUpdated(oldState _: Spotiny.PlayerState?, newState: Spotiny.PlayerState) {
        handleStateUpdate(newState, reason: "dealer")
    }

    public func serviceDidRateLimit(retryAfterSeconds: Int) {
        serviceHealthObserver?(.rateLimited(retryAfterSeconds: retryAfterSeconds))
    }

    public func serviceDidRecover() {
        serviceHealthObserver?(.ok)
    }
}

private extension LibraryItem {
    var artworkId: String? {
        switch self {
        case let .track(t): return t.image_id.isEmpty ? nil : t.image_id
        case let .playlist(p): return p.artworkId
        case let .podcastEpisode(e): return e.artworkId
        case let .show(s): return s.artworkId
        case let .station(s): return s.artworkId
        case .album, .artist: return nil
        }
    }
}
