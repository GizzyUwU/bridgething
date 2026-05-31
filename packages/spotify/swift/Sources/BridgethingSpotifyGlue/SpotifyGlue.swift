import BridgethingGateway
import BridgethingGlue
import BridgethingLyrics
import BridgethingSchema
import Foundation
import Spotiny
#if canImport(FoundationNetworking)
    import FoundationNetworking
#endif

public typealias WireRepeat = BridgethingSchema.RepeatMode
private typealias SpotinyRepeat = Spotiny.RepeatMode

private let assetIdPrefix = "spotify/img/"
private let hintDebounceNanos: UInt64 = 250_000_000
private let pollIntervalNanos: UInt64 = 60_000_000_000
private let spotifyAppBundle = "com.spotify.client"

/// Builds the refresh-only `OAuthAuthenticator` the host configured.
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
    private let usesDealer: Bool

    private var client: SpotinyClient?
    private var gateway: BridgethingGateway?
    private var heldScopes: Set<CompanionAuthorityScope> = []
    private var nowPlayingObserver: (@Sendable (GlueNowPlaying?) -> Void)?
    private var authObserver: (@Sendable (GlueAuthState) -> Void)?
    private var serviceHealthObserver: (@Sendable (GlueServiceHealth) -> Void)?
    private var hintFetchTask: Task<Void, Never>?
    private var baselinePollTask: Task<Void, Never>?
    private var connectTask: Task<Void, Never>?
    private var lastHintPid: String?

    public init(
        authenticatorFactory: @escaping SpotifyAuthenticatorFactory,
        accessToken: String = "",
        refreshToken: String = "",
        onTokensRefreshed: TokenCallback? = nil,
        urlSession: URLSession = .shared,
        httpExecutor: (any SpotinyHTTPExecutor)? = nil,
        usesDealer: Bool = false
    ) {
        self.authenticatorFactory = authenticatorFactory
        initialAccessToken = accessToken
        initialRefreshToken = refreshToken
        self.onTokensRefreshed = onTokensRefreshed
        self.urlSession = urlSession
        self.httpExecutor = httpExecutor
        self.usesDealer = usesDealer
    }

    public func attach(gateway: BridgethingGateway) async throws {
        if self.gateway != nil { await detach() }

        self.gateway = gateway

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
        let dealer = usesDealer
        connectTask = Task { [weak client] in
            if dealer {
                await client?.connect()
            } else {
                _ = await client?.authenticate()
            }
        }
    }

    public func detach() async {
        // Stop emitting auth state while tearing down: cancellation races in
        // spotiny would otherwise fire authDidFail and emit a ghost `failed`.
        authObserver = nil
        serviceHealthObserver = nil

        connectTask?.cancel()
        connectTask = nil
        hintFetchTask?.cancel()
        hintFetchTask = nil
        baselinePollTask?.cancel()
        baselinePollTask = nil

        await releaseAllAuthority()

        nowPlayingObserver?(nil)
        nowPlayingObserver = nil

        client = nil
        gateway = nil
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

        let limit = Int(req.limit)
        var items: [LibraryItem] = []
        var presentKinds: [ItemKind] = []
        var reachedFullPage = false

        for kind in kinds {
            let kindItems: [LibraryItem]
            switch kind {
            case .track: kindItems = results.tracks.map { .track(Self.mapTrack($0)) }
            case .album: kindItems = results.albums.map { .album(Self.mapAlbum($0)) }
            case .artist: kindItems = results.artists.map { .artist(Self.mapArtist($0)) }
            case .playlist: kindItems = results.playlists.map { .playlist(Self.mapPlaylist($0)) }
            case .show: kindItems = results.shows.map { .show(Self.mapShow($0)) }
            case .podcastEpisode: kindItems = results.episodes.map { .podcastEpisode(Self.mapEpisode($0)) }
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

        switch req.nodeId {
        case nil, "", "root":
            return await browseRoot(client)

        case Self.recentlyPlayedNode:
            // recently-played is cursor-based, not offset-paged.
            let entries = Self.dedupedTrackEntries(await client.player.getRecentlyPlayed(limit: 50))
            return BrowseResult(entries: entries, total: UInt32(entries.count), hasMore: false)

        case Self.topTracksNode:
            let page = await client.tracks.getUserTopTracks(limit: limit, offset: offset)
            let entries = page.items.map { BrowseEntry.item(.track(Self.mapTrack($0))) }
            return Self.section(entries, pageCount: page.items.count, total: page.total, offset: offset)

        case Self.homeNode:
            let page = await client.categories.getMadeForYou(limit: limit, offset: offset)
            let entries = page.items.map { BrowseEntry.item(.playlist(Self.mapPlaylist($0))) }
            return Self.section(entries, pageCount: page.items.count, total: page.total, offset: offset)

        case Self.playlistsNode:
            let page = await client.playlists.getUserPlaylists(limit: limit, offset: offset)
            var entries: [BrowseEntry] = []
            if offset == 0 {
                let userId = await client.users.getCurrentUser()?.id
                if let liked = Self.likedSongsEntry(userId: userId) { entries.append(liked) }
            }
            entries += page.items.map { BrowseEntry.item(.playlist(Self.mapPlaylist($0))) }
            return Self.section(entries, pageCount: page.items.count, total: page.total, offset: offset)

        case Self.podcastsNode:
            let page = await client.shows.getUserSavedShows(limit: limit, offset: offset)
            var entries: [BrowseEntry] = []
            if offset == 0 { entries.append(Self.yourEpisodesEntry()) }
            entries += page.items.map { BrowseEntry.item(.show(Self.mapShow($0))) }
            return Self.section(entries, pageCount: page.items.count, total: page.total, offset: offset)

        case Self.artistsNode:
            let page = await client.artists.getUserFollowedArtists(limit: limit, offset: offset)
            let entries = page.items.map { BrowseEntry.item(.artist(Self.mapArtist($0))) }
            return Self.section(entries, pageCount: page.items.count, total: page.total, offset: offset)

        case Self.albumsNode:
            let page = await client.albums.getUserSavedAlbums(limit: limit, offset: offset)
            let entries = page.items.map { BrowseEntry.item(.album(Self.mapAlbum($0))) }
            return Self.section(entries, pageCount: page.items.count, total: page.total, offset: offset)

        default:
            return BrowseResult(entries: [], total: nil, hasMore: false)
        }
    }

    /// Each section folder inlines a preview slice of its children.
    private func browseRoot(_ client: SpotinyClient) async -> BrowseResult {
        let previewLimit = 14

        async let recentP = client.player.getRecentlyPlayed(limit: previewLimit)
        async let topP = client.tracks.getUserTopTracks(limit: previewLimit)
        async let homeP = client.categories.getMadeForYou(limit: 10)
        async let playlistsP = client.playlists.getUserPlaylists(limit: previewLimit)
        async let showsP = client.shows.getUserSavedShows(limit: previewLimit)
        async let artistsP = client.artists.getUserFollowedArtists(limit: previewLimit)
        async let albumsP = client.albums.getUserSavedAlbums(limit: previewLimit)
        async let userP = client.users.getCurrentUser()

        let recentChildren = Self.dedupedTrackEntries(await recentP)
        let topChildren = (await topP).items.map { BrowseEntry.item(.track(Self.mapTrack($0))) }
        let homeChildren = (await homeP).items.map { BrowseEntry.item(.playlist(Self.mapPlaylist($0))) }

        var playlistChildren: [BrowseEntry] = []
        if let liked = Self.likedSongsEntry(userId: (await userP)?.id) { playlistChildren.append(liked) }
        playlistChildren += (await playlistsP).items.map { BrowseEntry.item(.playlist(Self.mapPlaylist($0))) }

        var podcastChildren: [BrowseEntry] = [Self.yourEpisodesEntry()]
        podcastChildren += (await showsP).items.map { BrowseEntry.item(.show(Self.mapShow($0))) }

        let artistChildren = (await artistsP).items.map { BrowseEntry.item(.artist(Self.mapArtist($0))) }
        let albumChildren = (await albumsP).items.map { BrowseEntry.item(.album(Self.mapAlbum($0))) }

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
        return BrowseResult(entries: folders, total: UInt32(folders.count), hasMore: false)
    }

    private static func section(_ entries: [BrowseEntry], pageCount: Int, total: Int, offset: Int) -> BrowseResult {
        BrowseResult(entries: entries, total: UInt32(max(total, 0)), hasMore: offset + pageCount < total)
    }

    // recently-played repeats tracks; dedupe by uri.
    private static func dedupedTrackEntries(_ tracks: [Spotiny.Track]) -> [BrowseEntry] {
        var seen = Set<String>()
        return tracks.compactMap { track in
            guard seen.insert(track.uri).inserted else { return nil }
            return .item(.track(Self.mapTrack(track)))
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

    public func recommendations(_ req: LibraryRecommendationsRequest) async throws -> RecommendationsResult {
        guard let client else { throw GlueError.detached }
        let limit = Int(req.limit)

        let seedTracks = req.seeds.filter { $0.kind == .track }.compactMap { SpotifyURI($0.uri)?.id }
        let seedArtists = req.seeds.filter { $0.kind == .artist }.compactMap { SpotifyURI($0.uri)?.id }

        let tracks = await client.recommendations.get(seedTracks: seedTracks, seedArtists: seedArtists, limit: limit)
        if !tracks.isEmpty {
            return RecommendationsResult(items: tracks.map { .track(Self.mapTrack($0)) }, total: nil, hasMore: false)
        }

        if let artistSeed = req.seeds.first(where: { $0.kind == .artist }), let uri = SpotifyURI(artistSeed.uri) {
            let top = Array((await client.artists.getArtistTopTracks(uri: uri)).prefix(limit))
            return RecommendationsResult(items: top.map { .track(Self.mapTrack($0)) }, total: nil, hasMore: false)
        }

        return RecommendationsResult(items: [], total: nil, hasMore: false)
    }

    public func favoritesList(_ req: LibraryFavoritesListRequest) async throws -> FavoritesPage {
        guard let client else { throw GlueError.detached }
        let offset = Int(req.offset)
        let page = await client.tracks.getUserSavedTracks(limit: Int(req.limit), offset: offset)
        let items = page.items.map { LibraryItem.track(Self.mapTrack($0, saved: true)) }
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
    }

    public func favoritesSet(_ item: ItemRef, liked: Bool) async throws {
        guard let client else { throw GlueError.detached }
        guard let uri = Self.spotifyURI(item.uri) else { throw GlueError.notImplemented }
        if liked {
            await client.library.save(uris: [uri])
        } else {
            await client.library.remove(uris: [uri])
        }
    }

    public func favoritesSetMany(_ entries: [FavoritesSet]) async throws {
        guard let client else { throw GlueError.detached }
        let toSave = entries.filter { $0.liked }.compactMap { Self.spotifyURI($0.item.uri) }
        let toRemove = entries.filter { !$0.liked }.compactMap { Self.spotifyURI($0.item.uri) }
        if !toSave.isEmpty { await client.library.save(uris: toSave) }
        if !toRemove.isEmpty { await client.library.remove(uris: toRemove) }
    }

    private static func spotifyURI(_ raw: String) -> SpotifyURI? {
        guard let uri = SpotifyURI(raw), uri.namespace == "spotify" else { return nil }
        return uri
    }

    public func handlePlaybackHint(_ hint: PlaybackHint) async {
        // Filter to Spotify-app hints only; other-app and unset-bundle hints drop.
        guard hint.appBundle == spotifyAppBundle else { return }

        // echo the iAP2 pid the daemon matches enrichment offers against; always the latest hint.
        lastHintPid = hint.persistentId

        DiagnosticsBuffer.shared.recordBreadcrumb(
            category: "spotify.merge",
            detail: "iap2 playback hint",
            fields: [("source", "iap2BackProp")]
        )
        hintFetchTask?.cancel()
        hintFetchTask = Task { [weak self] in
            try? await Task.sleep(nanoseconds: hintDebounceNanos)
            if Task.isCancelled { return }
            await self?.fetchAndDispatch(reason: "hint")
        }
    }

    public func debugState() async -> GlueDebugState {
        GlueDebugState(
            authorityPlaybackHeld: heldScopes.contains(.nowPlayingPlayback),
            authorityMetadataHeld: heldScopes.contains(.nowPlayingMetadata),
            baselinePollActive: baselinePollTask != nil,
            hintFetchActive: hintFetchTask != nil
        )
    }

    public func asset(id: String) async throws -> AssetBytes? {
        guard id.hasPrefix(assetIdPrefix) else { return nil }
        let encoded = String(id.dropFirst(assetIdPrefix.count))
        guard let urlString = encoded.removingPercentEncoding,
              let url = URL(string: urlString) else { return nil }
        let (data, response) = try await urlSession.data(from: url)
        let mime = (response as? HTTPURLResponse)?.value(forHTTPHeaderField: "Content-Type")
        return AssetBytes(bytes: data, mime: mime)
    }

    fileprivate func fetchAndDispatch(reason: String) async {
        guard let client else { return }
        guard let state = await client.player.getPlaybackState() else { return }
        handleStateUpdate(state, reason: reason)
    }

    // MARK: - outbound

    fileprivate func handleStateUpdate(_ state: Spotiny.PlayerState, reason: String) {
        guard let gateway else { return }
        let update = Self.makeUpdate(from: state)
        let artworkUrl = state.item.flatMap(Self.rawArtworkURL(for:))
        nowPlayingObserver?(GlueNowPlaying(update: update, artworkUrl: artworkUrl))

        let nowPlaying = state.is_playing
        DiagnosticsBuffer.shared.recordBreadcrumb(
            category: "spotify.merge",
            detail: "augmented now-playing",
            fields: [
                ("source", "companionAugmented"),
                ("reason", reason),
                ("track", state.item?.name ?? ""),
                ("playing", String(nowPlaying)),
            ]
        )

        #if os(iOS)
        let anchorPid = lastHintPid
        startBaselinePollIfNeeded()
        Task { [weak self] in
            await self?.sendEnrichment(state: state, anchorPid: anchorPid, reason: reason)
        }
        #else
        Task { [weak self] in
            try? await gateway.player.delta(update)
            guard let self else { return }
            let scopes: Set<CompanionAuthorityScope> = [.nowPlayingPlayback, .nowPlayingMetadata]
            if nowPlaying {
                await claimAuthority(scopes)
                startBaselinePollIfNeeded()
            } else if !heldScopes.isEmpty {
                await releaseAllAuthority()
                stopBaselinePoll()
            }
        }
        #endif
    }

    #if os(iOS)
    private func sendEnrichment(state: Spotiny.PlayerState, anchorPid: String?, reason: String) async {
        guard let gateway, let client else { return }
        let head = state.item.map(Self.queueItem(from:))
        let queue = (await client.player.getQueue())?.queue.map(Self.queueItem(from:)) ?? []
        let context = state.context.map { EnrichmentContext(uri: $0.uri, name: nil, kind: $0.type) }
        let offer = NowPlayingEnrichment(anchorPid: anchorPid, head: head, queue: queue, context: context)

        DiagnosticsBuffer.shared.recordBreadcrumb(
            category: "spotify.merge",
            detail: "enrichment offer",
            fields: [
                ("reason", reason),
                ("anchor", anchorPid ?? ""),
                ("head", state.item?.name ?? ""),
                ("queue", String(queue.count)),
            ]
        )
        try? await gateway.player.enrichmentOffer(offer)
    }
    #endif

    private func claimAuthority(_ scopes: Set<CompanionAuthorityScope>) async {
        guard let gateway else { return }
        for scope in scopes where !heldScopes.contains(scope) {
            try? await gateway.authority.claim(AuthorityClaim(scope: scope))
            heldScopes.insert(scope)
            DiagnosticsBuffer.shared.recordBreadcrumb(
                category: "spotify.authority", detail: "claimed \(scope.rawValue)"
            )
        }
    }

    private func releaseAllAuthority() async {
        guard let gateway else { return }
        for scope in heldScopes {
            try? await gateway.authority.release(AuthorityRelease(scope: scope))
            DiagnosticsBuffer.shared.recordBreadcrumb(
                category: "spotify.authority", detail: "released \(scope.rawValue)"
            )
        }
        heldScopes.removeAll()
    }

    fileprivate func handleSocketDown() {
        nowPlayingObserver?(nil)
        stopBaselinePoll()
        guard gateway != nil, !heldScopes.isEmpty else { return }
        Task { await releaseAllAuthority() }
    }

    private func startBaselinePollIfNeeded() {
        guard baselinePollTask == nil else { return }
        baselinePollTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: pollIntervalNanos)
                if Task.isCancelled { return }
                await self?.fetchAndDispatch(reason: "poll")
            }
        }
    }

    private func stopBaselinePoll() {
        baselinePollTask?.cancel()
        baselinePollTask = nil
    }

    private static func makeUpdate(from state: Spotiny.PlayerState) -> NowPlayingUpdate {
        let media: MediaItemUpdate? = state.item.map { item in
            let title = item.name
            let artist = item.artists.map(\.name).joined(separator: ", ")
            let album: String? = if case let .track(track) = item { track.album?.name } else { nil }
            return MediaItemUpdate(
                persistentId: item.uri,
                title: title.isEmpty ? nil : title,
                album: album,
                albumArtist: nil,
                artist: artist.isEmpty ? nil : artist,
                liked: nil,
                artworkId: artworkId(for: item),
                durationMs: UInt32(max(item.duration_ms, 0)),
                mediaTypes: nil,
                trackNumber: nil,
                trackCount: nil,
                isLikeSupported: nil,
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

    private static func artworkId(for item: PlayerItem) -> String? {
        imageAssetId(rawArtworkURL(for: item) ?? "")
    }

    private static func queueItem(from item: PlayerItem) -> QueueItem {
        let artist = item.artists.map(\.name).joined(separator: ", ")
        let album: String? = if case let .track(track) = item { track.album?.name } else { nil }
        return QueueItem(
            uri: item.uri,
            title: item.name.isEmpty ? nil : item.name,
            artist: artist.isEmpty ? nil : artist,
            album: album,
            artworkId: artworkId(for: item),
            durationMs: UInt32(max(item.duration_ms, 0)),
            persistentId: nil
        )
    }

    fileprivate static func rawArtworkURL(for item: PlayerItem) -> String? {
        let url = bestImageURL(item.imageUrl)
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

    private static func bestImageURL(_ urls: SpotifyImageURLs) -> String {
        if !urls.large.isEmpty { return urls.large }
        if !urls.medium.isEmpty { return urls.medium }
        if !urls.small.isEmpty { return urls.small }
        return ""
    }

    private static func imageAssetId(_ rawURL: String) -> String? {
        guard !rawURL.isEmpty,
              let encoded = rawURL.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed)
        else { return nil }
        return assetIdPrefix + encoded
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

    private static func mapTrack(_ t: Spotiny.Track, saved: Bool = false) -> BridgethingSchema.Track {
        let primary = t.artists.first
        return BridgethingSchema.Track(
            id: t.uri,
            name: t.name,
            album: BridgethingSchema.Album(id: t.album?.uri ?? "", name: t.album?.name ?? ""),
            artist: BridgethingSchema.Artist(id: primary?.uri ?? "", name: primary?.name ?? ""),
            artists: t.artists.map { BridgethingSchema.Artist(id: $0.uri, name: $0.name) },
            duration_ms: UInt32(max(t.duration_ms, 0)),
            image_id: imageAssetId(bestImageURL(t.imageUrl)) ?? "",
            saved: saved
        )
    }

    private static func mapAlbum(_ a: Spotiny.Album) -> BridgethingSchema.Album {
        BridgethingSchema.Album(id: a.uri, name: a.name)
    }

    private static func mapArtist(_ a: Spotiny.Artist) -> BridgethingSchema.Artist {
        BridgethingSchema.Artist(id: a.uri, name: a.name)
    }

    private static func mapPlaylist(_ p: Spotiny.Playlist) -> BridgethingSchema.Playlist {
        BridgethingSchema.Playlist(
            uri: p.uri,
            name: p.name,
            ownerName: nil,
            trackCount: nil,
            artworkId: imageAssetId(bestImageURL(p.imageUrl))
        )
    }

    private static func mapShow(_ s: Spotiny.Show) -> BridgethingSchema.Show {
        BridgethingSchema.Show(
            uri: s.uri,
            name: s.name,
            publisher: nil,
            episodeCount: nil,
            artworkId: imageAssetId(bestImageURL(s.imageUrl))
        )
    }

    private static func mapEpisode(_ e: Spotiny.Episode) -> BridgethingSchema.PodcastEpisode {
        BridgethingSchema.PodcastEpisode(
            uri: e.uri,
            name: e.name,
            showName: e.show?.name,
            durationMs: UInt32(max(e.duration_ms, 0)),
            publishedAtUnixS: nil,
            artworkId: imageAssetId(bestImageURL(e.imageUrl))
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
