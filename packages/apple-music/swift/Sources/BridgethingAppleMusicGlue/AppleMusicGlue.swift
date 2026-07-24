import BridgethingGateway
import BridgethingGlue
import BridgethingSchema
import Foundation
import os
#if canImport(UIKit)
    import UIKit
#endif

public typealias WireRepeat = BridgethingSchema.RepeatMode

private typealias LibraryItem = BridgethingSchema.LibraryItem

private let appleMusicAppBundle = "com.apple.Music"
private let imageCodec = ImageAssetCodec(namespace: "applemusic/img/")
private let uriPrefix = "applemusic:"
private let recNodePrefix = "rec:"
private let playlistsNodeId = "playlists"
private let albumsNodeId = "albums"
private let artistsNodeId = "artists"
private let songsNodeId = "songs"
private let recentsNodeId = "recently-played"
private let defaultHeroEdge = 248
private let defaultThumbEdge = 96
private let defaultRootPreview: UInt32 = 8
private let glueLog = Logger(subsystem: "com.bridgething.applemusic", category: "glue")

public enum AmUri {
    public static func make(_ kind: AmKind, id: String) -> String {
        "\(uriPrefix)\(kind.rawValue):\(id)"
    }

    public static func parse(_ uri: String) -> (kind: AmKind, id: String)? {
        guard uri.hasPrefix(uriPrefix) else { return nil }
        let rest = uri.dropFirst(uriPrefix.count)
        guard let colon = rest.firstIndex(of: ":"), let kind = AmKind(rawValue: String(rest[..<colon])) else { return nil }
        let id = String(rest[rest.index(after: colon)...])
        return id.isEmpty ? nil : (kind, id)
    }
}

func sizedArtworkUrl(_ template: String, edge: Int) -> String {
    template
        .replacingOccurrences(of: "{w}", with: String(edge))
        .replacingOccurrences(of: "{h}", with: String(edge))
}

public final class AppleMusicGlue: BridgethingGlue, @unchecked Sendable {
    public static let name: String = "apple-music"
    public static let displayName: String = "Apple Music"

    public let capabilities: GlueCapabilities = [
        .streaming, .albumArt, .recommendations, .recentlyPlayed, .library, .playlists,
    ]
    public let uriSchemes: [String] = ["applemusic"]
    public let musicProvider: MusicProvider = .appleMusic
    public let lyricsSupported: Bool = false

    private let auth: any AppleMusicAuthProviding
    private let player: any AppleMusicPlayerProviding
    private let library: any AppleMusicLibraryProviding
    private let urlSession: URLSession

    private let stateLock = NSLock()
    private var gateway: BridgethingGateway?
    private var heldScopes: Set<CompanionAuthorityScope> = []
    private var nowPlayingObserver: (@Sendable (GlueNowPlaying?) -> Void)?
    private var authObserver: (@Sendable (GlueAuthState) -> Void)?
    private var serviceHealthObserver: (@Sendable (GlueServiceHealth) -> Void)?
    private var lastSnapshot: AmPlayerSnapshot?
    private var likedCache: [String: Bool] = [:]
    private var likedFetchUri: String?
    private var artHeroEdge = defaultHeroEdge
    private var artThumbEdge = defaultThumbEdge
    private var authorized = false

    private var authTask: Task<Void, Never>?
    private var observeTask: Task<Void, Never>?
    private var foregroundTask: Task<Void, Never>?
    private var likedTask: Task<Void, Never>?
    private var emitTask: Task<Void, Never>?
    private var emitContinuation: AsyncStream<EmitJob>.Continuation?

    private enum EmitJob {
        case player(snapshot: BridgethingSchema.PlayerState, hasItem: Bool)
    }

    public static let defaultImageSession: URLSession = {
        let cfg = URLSessionConfiguration.default
        cfg.timeoutIntervalForRequest = 6
        cfg.timeoutIntervalForResource = 15
        let artDir = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)
            .first?
            .appendingPathComponent("AppleMusicArt", isDirectory: true)
        cfg.urlCache = URLCache(memoryCapacity: 8 << 20, diskCapacity: 200 << 20, directory: artDir)
        cfg.requestCachePolicy = .returnCacheDataElseLoad
        return URLSession(configuration: cfg)
    }()

    public init(
        auth: (any AppleMusicAuthProviding)? = nil,
        player: (any AppleMusicPlayerProviding)? = nil,
        library: (any AppleMusicLibraryProviding)? = nil,
        urlSession: URLSession = AppleMusicGlue.defaultImageSession
    ) {
        self.auth = auth ?? Self.defaultAuth()
        self.player = player ?? Self.defaultPlayer()
        self.library = library ?? Self.defaultLibrary()
        self.urlSession = urlSession
    }

    // MARK: - synchronized handles

    private func currentGateway() -> BridgethingGateway? { stateLock.withLock { gateway } }
    private func setGateway(_ g: BridgethingGateway?) { stateLock.withLock { gateway = g } }
    private func currentNowPlayingObserver() -> (@Sendable (GlueNowPlaying?) -> Void)? {
        stateLock.withLock { nowPlayingObserver }
    }

    private func currentAuthObserver() -> (@Sendable (GlueAuthState) -> Void)? {
        stateLock.withLock { authObserver }
    }

    // MARK: - lifecycle

    public func attach(gateway: BridgethingGateway) async throws {
        if currentGateway() != nil { await detach() }
        setGateway(gateway)
        startEmitter()

        currentAuthObserver()?(.pending(nil))
        authTask = Task { [weak self] in
            await self?.runAuth()
        }
    }

    private func runAuth() async {
        var status = await auth.currentStatus()
        if status == .notDetermined {
            status = await auth.requestAuthorization()
        }
        guard status == .authorized else {
            glueLog.warning("media library authorization: \(String(describing: status), privacy: .public)")
            currentAuthObserver()?(.failed("Apple Music access is not allowed. Enable it in Settings > Privacy > Media & Apple Music."))
            return
        }
        if await auth.canPlayCatalogContent() == false {
            currentAuthObserver()?(.failed("An Apple Music subscription is required"))
            return
        }
        stateLock.withLock { authorized = true }
        currentAuthObserver()?(.authenticated)
        glueLog.info("authorized; starting player observation")
        startObservation()
        #if canImport(UIKit)
            foregroundTask = Task { [weak self] in
                for await _ in NotificationCenter.default.notifications(named: UIApplication.didBecomeActiveNotification) {
                    self?.startObservation()
                }
            }
        #endif
    }

    private func startObservation() {
        observeTask?.cancel()
        observeTask = Task { [weak self] in
            guard let self else { return }
            if await self.player.currentSnapshot().entry != nil {
                await self.emitCurrent()
            }
            for await _ in self.player.changes() {
                if Task.isCancelled { return }
                await self.emitCurrent()
            }
        }
    }

    public func detach() async {
        stateLock.withLock {
            authObserver = nil
            serviceHealthObserver = nil
            authorized = false
        }
        authTask?.cancel()
        authTask = nil
        observeTask?.cancel()
        observeTask = nil
        foregroundTask?.cancel()
        foregroundTask = nil
        likedTask?.cancel()
        likedTask = nil
        await releaseAllAuthority()
        let npObs = stateLock.withLock { nowPlayingObserver }
        npObs?(nil)
        stopEmitter()
        stateLock.withLock {
            nowPlayingObserver = nil
            lastSnapshot = nil
            likedCache.removeAll()
            likedFetchUri = nil
        }
        setGateway(nil)
    }

    // MARK: - serial emitter

    private func startEmitter() {
        let (stream, cont) = AsyncStream<EmitJob>.makeStream()
        stateLock.withLock { emitContinuation = cont }
        emitTask = Task { [weak self] in
            for await job in stream {
                guard let self else { break }
                await self.handleEmit(job)
            }
        }
    }

    private func stopEmitter() {
        let cont = stateLock.withLock { () -> AsyncStream<EmitJob>.Continuation? in
            let c = emitContinuation
            emitContinuation = nil
            return c
        }
        cont?.finish()
        emitTask?.cancel()
        emitTask = nil
    }

    private func enqueue(_ job: EmitJob) {
        stateLock.withLock { emitContinuation }?.yield(job)
    }

    private func handleEmit(_ job: EmitJob) async {
        guard let gateway = currentGateway() else { return }
        switch job {
        case let .player(snapshot, hasItem):
            if hasItem {
                await claimAuthority([.nowPlayingPlayback, .nowPlayingMetadata])
            } else {
                await releaseAllAuthority()
            }
            try? await gateway.player.snapshot(snapshot)
        }
    }

    // MARK: - observation -> snapshots

    private func emitCurrent() async {
        guard stateLock.withLock({ authorized }), currentGateway() != nil else { return }
        let snap = await player.currentSnapshot()
        stateLock.withLock { lastSnapshot = snap }
        let heroEdge = artEdges().hero
        let liked = likeFields(for: snap.entry)
        currentNowPlayingObserver()?(GlueNowPlaying(
            update: makeUpdate(from: snap, heroEdge: heroEdge, liked: liked),
            artworkUrl: snap.entry?.artworkUrl.map { sizedArtworkUrl($0, edge: heroEdge) }
        ))
        enqueue(.player(
            snapshot: makeSnapshot(from: snap, heroEdge: heroEdge, liked: liked),
            hasItem: snap.entry != nil
        ))
        refreshLikedIfNeeded(for: snap.entry)
    }

    private func likeFields(for entry: AmEntry?) -> Bool? {
        guard let uri = entry?.uri else { return nil }
        return stateLock.withLock { likedCache[uri] }
    }

    private func refreshLikedIfNeeded(for entry: AmEntry?) {
        guard let uri = entry?.uri else { return }
        let needsFetch = stateLock.withLock { () -> Bool in
            guard likedCache[uri] == nil, likedFetchUri != uri else { return false }
            likedFetchUri = uri
            return true
        }
        guard needsFetch else { return }
        likedTask = Task { [weak self] in
            guard let self else { return }
            guard let fav = try? await self.library.isFavorite(uris: [uri]).first else { return }
            let stillCurrent = self.stateLock.withLock { () -> Bool in
                self.likedCache[uri] = fav
                return self.lastSnapshot?.entry?.uri == uri
            }
            if stillCurrent { await self.emitCurrent() }
        }
    }

    // MARK: - observers

    public func setNowPlayingObserver(_ observer: @escaping @Sendable (GlueNowPlaying?) -> Void) async {
        stateLock.withLock { nowPlayingObserver = observer }
    }

    public func setAuthObserver(_ observer: @escaping @Sendable (GlueAuthState) -> Void) async {
        stateLock.withLock { authObserver = observer }
    }

    public func setServiceHealthObserver(_ observer: @escaping @Sendable (GlueServiceHealth) -> Void) async {
        stateLock.withLock { serviceHealthObserver = observer }
        observer(.ok)
    }

    public func setArtProfile(heroPx: Int, thumbPx: Int) {
        stateLock.withLock {
            artHeroEdge = max(1, heroPx)
            artThumbEdge = max(1, thumbPx)
        }
    }

    private func artEdges() -> (hero: Int, thumb: Int) {
        stateLock.withLock { (artHeroEdge, artThumbEdge) }
    }

    public func handlePeerConnected(allowAutoResume: Bool) async {
        guard currentGateway() != nil else { return }
        stateLock.withLock { heldScopes.removeAll() }
        if stateLock.withLock({ authorized && lastSnapshot?.entry != nil }) {
            await emitCurrent()
        }
        guard allowAutoResume, stateLock.withLock({ authorized }) else { return }
        let snap = stateLock.withLock { lastSnapshot }
        if snap?.playing == true { return }
        if await player.isOtherAudioPlaying() {
            glueLog.info("peer connect: other audio active; not resuming")
            return
        }
        glueLog.info("peer connect: resuming Music app playback")
        do { try await player.play() } catch {
            glueLog.info("connect auto-resume did not complete: \(String(describing: error), privacy: .public)")
        }
    }

    public func debugState() async -> GlueDebugState {
        stateLock.withLock {
            GlueDebugState(
                authorityPlaybackHeld: heldScopes.contains(.nowPlayingPlayback),
                authorityMetadataHeld: heldScopes.contains(.nowPlayingMetadata)
            )
        }
    }

    // MARK: - inbound transport

    public func play(_ uri: PlayUri) async throws {
        if let ctx = uri.context {
            try await player.play(contextUri: ctx.contextUri, startAtUri: uri.uri)
        } else {
            try await player.play(contextUri: uri.uri, startAtUri: nil)
        }
    }

    public func queue(_ req: QueueUri) async throws {
        switch req.position {
        case .append: try await player.queueInsert(uri: req.uri, next: false)
        case .next: try await player.queueInsert(uri: req.uri, next: true)
        case .index: throw GlueError.notImplemented
        }
    }

    public func pause() async throws { try await player.pause() }
    public func resume() async throws { try await player.play() }
    public func skipNext() async throws { try await player.skipNext() }
    public func skipPrev() async throws { try await player.skipPrev() }
    public func seekTo(_ ms: UInt32) async throws { try await player.seek(toMs: ms) }
    public func setShuffle(_ on: Bool) async throws { try await player.setShuffle(on) }

    public func setRepeat(_ mode: WireRepeat) async throws {
        let mapped: AmRepeatMode = switch mode {
        case .off: .off
        case .all: .all
        case .one: .one
        }
        try await player.setRepeat(mapped)
    }

    // MARK: - library

    public func browse(_ req: LibraryBrowseRequest) async throws -> BrowseResult {
        let result: BrowseResult
        switch req.nodeId {
        case nil, "", "root":
            result = try await rootBrowse(sections: req.sections, preview: req.preview)
        case playlistsNodeId:
            result = pageResult(try await library.libraryPlaylists(limit: req.limit, offset: req.offset))
        case albumsNodeId:
            result = pageResult(try await library.libraryAlbums(limit: req.limit, offset: req.offset))
        case artistsNodeId:
            result = pageResult(try await library.libraryArtists(limit: req.limit, offset: req.offset))
        case songsNodeId:
            result = pageResult(try await library.librarySongs(limit: req.limit, offset: req.offset))
        case recentsNodeId:
            result = pageResult(try await library.recentlyPlayed(limit: req.limit, offset: req.offset))
        case let node? where node.hasPrefix(recNodePrefix):
            let railId = String(node.dropFirst(recNodePrefix.count))
            guard let shelf = try await library.recommendations().first(where: { $0.id == railId }) else {
                return BrowseResult(entries: [], total: 0, hasMore: false)
            }
            let page = shelf.items.dropFirst(Int(req.offset)).prefix(Int(req.limit))
            result = BrowseResult(
                entries: page.compactMap { self.libraryItem($0).map { BrowseEntry.item($0) } },
                total: shelf.total ?? UInt32(shelf.items.count),
                hasMore: Int(req.offset) + page.count < shelf.items.count
            )
        case let node?:
            result = pageResult(try await library.children(of: node, limit: req.limit, offset: req.offset))
        }
        warmArt(in: result)
        return result
    }

    private func rootBrowse(sections: UInt32?, preview: UInt32?) async throws -> BrowseResult {
        let previewCount = preview ?? defaultRootPreview
        var folders: [BrowseFolder] = []
        let staples: [(String, String)] = [
            (playlistsNodeId, "Playlists"),
            (albumsNodeId, "Albums"),
            (artistsNodeId, "Artists"),
            (songsNodeId, "Songs"),
        ]
        for (nodeId, title) in staples {
            if previewCount == 0 {
                folders.append(BrowseFolder(nodeId: nodeId, title: title, subtitle: nil, artworkId: nil, total: nil, previewChildren: nil))
                continue
            }
            let page = (try? await fetchStaple(nodeId, limit: previewCount)) ?? AmPage(items: [], total: nil, hasMore: false)
            let children = page.items.compactMap { libraryItem($0).map { BrowseEntry.item($0) } }
            folders.append(BrowseFolder(
                nodeId: nodeId, title: title, subtitle: nil, artworkId: nil,
                total: page.total, previewChildren: children.isEmpty ? nil : children
            ))
        }
        if let rails = try? await library.recommendations() {
            for rail in rails {
                let children = previewCount == 0 ? [] : rail.items.prefix(Int(previewCount)).compactMap { libraryItem($0).map { BrowseEntry.item($0) } }
                folders.append(BrowseFolder(
                    nodeId: recNodePrefix + rail.id, title: rail.title, subtitle: nil, artworkId: nil,
                    total: rail.total ?? UInt32(rail.items.count),
                    previewChildren: children.isEmpty ? nil : children
                ))
            }
        }
        if let sections { folders = Array(folders.prefix(Int(sections))) }
        return BrowseResult(entries: folders.map { .folder($0) }, total: UInt32(folders.count), hasMore: false)
    }

    private func fetchStaple(_ nodeId: String, limit: UInt32) async throws -> AmPage {
        switch nodeId {
        case playlistsNodeId: try await library.libraryPlaylists(limit: limit, offset: 0)
        case albumsNodeId: try await library.libraryAlbums(limit: limit, offset: 0)
        case artistsNodeId: try await library.libraryArtists(limit: limit, offset: 0)
        case songsNodeId: try await library.librarySongs(limit: limit, offset: 0)
        default: AmPage(items: [], total: nil, hasMore: false)
        }
    }

    private func pageResult(_ page: AmPage) -> BrowseResult {
        BrowseResult(
            entries: page.items.compactMap { libraryItem($0).map { BrowseEntry.item($0) } },
            total: page.total,
            hasMore: page.hasMore
        )
    }

    public func search(_ req: LibrarySearchRequest) async throws -> SearchResult {
        let kinds = (req.kinds?.isEmpty == false) ? req.kinds! : [.track, .album, .artist, .playlist]
        let res = try await library.search(query: req.query, limit: req.limit)
        let limit = Int(req.limit)
        var items: [LibraryItem] = []
        var present: [ItemKind] = []
        var full = false
        for kind in kinds {
            let arr: [AmItem]
            switch kind {
            case .track: arr = res.songs
            case .album: arr = res.albums
            case .artist: arr = res.artists
            case .playlist: arr = res.playlists
            default: arr = []
            }
            let mapped = arr.compactMap { libraryItem($0) }
            if !mapped.isEmpty {
                present.append(kind)
                if mapped.count >= limit { full = true }
            }
            items.append(contentsOf: mapped)
        }
        return SearchResult(items: items, kinds: present, total: nil, hasMore: full)
    }

    public func resolveContext(_ uri: String) async throws -> ContextResolveReply {
        let item = try await library.resolve(uri: uri)
        return ContextResolveReply(
            name: item.title.isEmpty ? nil : item.title,
            artworkId: artAssetId(item.artworkUrl, edge: artEdges().hero),
            subtitle: item.subtitle
        )
    }

    public func recommendations(_ req: LibraryRecommendationsRequest) async throws -> RecommendationsResult {
        if let artist = req.seeds.first(where: { $0.kind == .artist }) {
            let page = try await library.children(of: artist.uri, limit: req.limit, offset: req.offset)
            return RecommendationsResult(items: page.items.compactMap { libraryItem($0) }, total: page.total, hasMore: page.hasMore)
        }
        return RecommendationsResult(items: [], total: nil, hasMore: false)
    }

    // apple's favorites api is add-only: the star can be set but never removed

    public func favoritesContains(_ req: LibraryFavoritesContainsRequest) async throws -> [Bool] {
        try await library.isFavorite(uris: req.uris)
    }

    public func favoritesToggle(_ item: ItemRef) async throws {
        let cached = stateLock.withLock { likedCache[item.uri] }
        let current: Bool
        if let cached {
            current = cached
        } else {
            current = (try await library.isFavorite(uris: [item.uri])).first ?? false
        }
        guard !current else { throw GlueError.notImplemented }
        try await library.addFavorite(uri: item.uri)
        await applyLikedChange(uri: item.uri, liked: true)
    }

    public func favoritesSet(_ item: ItemRef, liked: Bool) async throws {
        guard liked else { throw GlueError.notImplemented }
        try await library.addFavorite(uri: item.uri)
        await applyLikedChange(uri: item.uri, liked: true)
    }

    public func favoritesSetMany(_ entries: [FavoritesSet]) async throws {
        for entry in entries {
            guard entry.liked else {
                glueLog.info("skipping unfavorite for \(entry.item.uri, privacy: .public): apple music favorites are add-only")
                continue
            }
            try await library.addFavorite(uri: entry.item.uri)
            await applyLikedChange(uri: entry.item.uri, liked: true)
        }
    }

    private func applyLikedChange(uri: String, liked: Bool) async {
        let isCurrent = stateLock.withLock { () -> Bool in
            likedCache[uri] = liked
            return lastSnapshot?.entry?.uri == uri
        }
        if isCurrent { await emitCurrent() }
    }

    // MARK: - assets

    func artSession(for url: URL) -> URLSession {
        switch url.scheme?.lowercased() {
        case "http", "https": urlSession
        default: .shared
        }
    }

    public func asset(id: String) async throws -> AssetBytes? {
        guard let parsed = imageCodec.parse(id) else { return nil }
        let (data, _) = try await artSession(for: parsed.url).data(from: parsed.url)
        return autoreleasepool {
            ArtImage.downsampleJpeg(data, maxEdge: parsed.maxEdge).map { AssetBytes(bytes: $0, mime: "image/jpeg") }
        }
    }

    private func warmArt(in result: BrowseResult) {
        for id in Set(collectArtIds(result.entries)) {
            guard let parsed = imageCodec.parse(id) else { continue }
            let session = artSession(for: parsed.url)
            Task { _ = try? await session.data(from: parsed.url) }
        }
    }

    private func collectArtIds(_ entries: [BrowseEntry]) -> [String] {
        entries.flatMap { entry -> [String] in
            switch entry {
            case let .folder(folder):
                return (folder.artworkId.map { [$0] } ?? []) + collectArtIds(folder.previewChildren ?? [])
            case let .item(item):
                return item.artworkId.map { [$0] } ?? []
            }
        }
    }

    private func artAssetId(_ template: String?, edge: Int) -> String? {
        guard let template, !template.isEmpty else { return nil }
        return imageCodec.assetId(url: sizedArtworkUrl(template, edge: edge), maxEdge: edge)
    }

    // MARK: - authority

    private func claimAuthority(_ scopes: Set<CompanionAuthorityScope>) async {
        guard let gateway = currentGateway() else { return }
        for scope in scopes {
            let needs = stateLock.withLock { !heldScopes.contains(scope) }
            if needs {
                do {
                    try await gateway.authority.claim(AuthorityClaim(scope: scope, appBundle: appleMusicAppBundle))
                    stateLock.withLock { _ = heldScopes.insert(scope) }
                } catch {}
            }
        }
    }

    private func releaseAllAuthority() async {
        guard let gateway = currentGateway() else { return }
        let scopes = stateLock.withLock { () -> [CompanionAuthorityScope] in
            let s = Array(heldScopes)
            heldScopes.removeAll()
            return s
        }
        for scope in scopes { try? await gateway.authority.release(AuthorityRelease(scope: scope)) }
    }

    // MARK: - wire mapping

    private func makeSnapshot(from snap: AmPlayerSnapshot, heroEdge: Int, liked: Bool?) -> BridgethingSchema.PlayerState {
        let track: MediaItem? = snap.entry.map { e in
            MediaItem(
                uri: e.uri ?? "",
                persistentId: e.uri,
                title: e.title.isEmpty ? nil : e.title,
                album: e.albumName,
                albumUri: nil,
                albumArtist: nil,
                artist: e.artistName,
                artistUri: nil,
                liked: liked,
                artworkId: artAssetId(e.artworkUrl, edge: heroEdge),
                durationMs: e.durationMs,
                mediaTypes: nil,
                trackNumber: nil,
                trackCount: nil,
                isLikeSupported: e.uri != nil,
                isBanSupported: nil,
                isBanned: nil,
                chapterCount: nil
            )
        }
        let playback = Playback(
            state: snap.playing ? .playing : .paused,
            positionMs: snap.positionMs,
            positionAgeMs: nil,
            shuffle: snap.shuffle,
            shuffleMode: snap.shuffle ? .songs : .off,
            repeat: mapRepeat(snap.repeatMode),
            queueIndex: nil,
            queueCount: nil,
            queueChapterIndex: nil,
            setElapsedTimeAvailable: snap.canSeek,
            queueListAvail: nil,
            appleMusicRadioAd: nil
        )
        return BridgethingSchema.PlayerState(
            track: track, playback: playback, queue: [],
            options: PlayerOptions(speed: 1.0, crossfade_ms: nil), context: nil
        )
    }

    private func makeUpdate(from snap: AmPlayerSnapshot, heroEdge: Int, liked: Bool?) -> NowPlayingUpdate {
        let media: MediaItemUpdate? = snap.entry.map { e in
            MediaItemUpdate(
                persistentId: e.uri,
                title: e.title.isEmpty ? nil : e.title,
                album: e.albumName,
                albumUri: nil,
                albumArtist: nil,
                artist: e.artistName,
                artistUri: nil,
                liked: liked,
                artworkId: artAssetId(e.artworkUrl, edge: heroEdge),
                durationMs: e.durationMs,
                mediaTypes: nil,
                trackNumber: nil,
                trackCount: nil,
                isLikeSupported: e.uri != nil,
                isBanSupported: nil,
                isBanned: nil,
                isResidentOnDevice: nil,
                chapterCount: nil
            )
        }
        let playback = PlaybackUpdate(
            playing: snap.playing,
            positionMs: snap.positionMs,
            shuffle: snap.shuffle,
            shuffleMode: snap.shuffle ? .songs : .off,
            repeat: mapRepeat(snap.repeatMode),
            appBundle: appleMusicAppBundle,
            appDisplayName: "Apple Music",
            queueIndex: nil,
            queueCount: nil,
            queueChapterIndex: nil,
            playbackSpeed: nil,
            setElapsedTimeAvailable: snap.canSeek,
            queueListAvail: nil,
            appleMusicRadioAd: nil,
            appleMusicRadioStationName: nil
        )
        return NowPlayingUpdate(mediaItem: media, playback: playback)
    }

    private func mapRepeat(_ mode: AmRepeatMode) -> WireRepeat {
        switch mode {
        case .off: .off
        case .all: .all
        case .one: .one
        }
    }

    private func libraryItem(_ item: AmItem) -> LibraryItem? {
        let art = artAssetId(item.artworkUrl, edge: artEdges().hero)
        switch item.kind {
        case .song:
            return .track(BridgethingSchema.Track(
                id: item.uri,
                name: item.title,
                album: BridgethingSchema.Album(id: item.albumUri ?? "", name: item.albumName ?? "", artwork_id: nil),
                artist: BridgethingSchema.Artist(id: item.artistUri ?? "", name: item.artistName ?? "", artwork_id: nil),
                artists: item.artistName.map { [BridgethingSchema.Artist(id: item.artistUri ?? "", name: $0, artwork_id: nil)] } ?? [],
                duration_ms: item.durationMs ?? 0,
                image_id: artAssetId(item.artworkUrl, edge: artEdges().thumb) ?? "",
                saved: false
            ))
        case .album:
            return .album(BridgethingSchema.Album(id: item.uri, name: item.title, artwork_id: art))
        case .artist:
            return .artist(BridgethingSchema.Artist(id: item.uri, name: item.title, artwork_id: art))
        case .playlist:
            return .playlist(Playlist(uri: item.uri, name: item.title, ownerName: item.subtitle, trackCount: item.trackCount, artworkId: art))
        case .station:
            return .station(Station(uri: item.uri, name: item.title, seed: nil, artworkId: art))
        }
    }
}
