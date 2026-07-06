import BridgethingGateway
import BridgethingGlue
import BridgethingSchema
import Foundation
import os
@_exported import Spotify
#if canImport(ImageIO)
    import CoreGraphics
    import ImageIO
#endif
#if canImport(UIKit)
    import UIKit
#endif

public typealias WireRepeat = BridgethingSchema.RepeatMode

private typealias LibraryItem = BridgethingSchema.LibraryItem

private typealias SpPlayerState = Spotify.PlayerState
private typealias SpTrack = Spotify.Track
private typealias SpQueue = Spotify.Queue
private typealias SpDevice = Spotify.Device
private typealias SpAuthState = Spotify.AuthState
private typealias SpBrowseItem = Spotify.BrowseItem
private typealias SpShelf = Spotify.Shelf
private typealias SpLibraryScope = Spotify.LibraryScope

private let scdnImagePrefix = "https://i.scdn.co/image/"
private let assetIdPrefix = "spotify/img/"
private let defaultHeroEdge = 248
private let defaultThumbEdge = 96
private let queueMax = 50
private let queueRunwayFloor = 8
private let spotifyAppBundle = "com.spotify.client"
private let glueLog = Logger(subsystem: "com.bridgething.spotify", category: "glue")

public final class SpotifyGlue: BridgethingGlue, @unchecked Sendable {
    public static let name: String = "spotify"
    public static let displayName: String = "Spotify"

    public let capabilities: GlueCapabilities = [
        .streaming, .queue, .albumArt, .recommendations, .recentlyPlayed, .library, .playlists,
    ]
    public let uriSchemes: [String] = ["spotify"]
    public let musicProvider: MusicProvider = .spotify
    public let lyricsSupported: Bool = false

    private let workerBase: String
    private let psk: String
    private let deviceId: String
    private let tokenStore: any Spotify.TokenStore
    private let clientFactory: SpotifyClientFactory
    private let urlSession: URLSession

    private var client: (any SpotifyClientProviding)?
    private var gateway: BridgethingGateway?
    private var connectTask: Task<Void, Never>?
    private var foregroundTask: Task<Void, Never>?

    private let stateLock = NSLock()
    private var heldScopes: Set<CompanionAuthorityScope> = []
    private var nowPlayingObserver: (@Sendable (GlueNowPlaying?) -> Void)?
    private var authObserver: (@Sendable (GlueAuthState) -> Void)?
    private var serviceHealthObserver: (@Sendable (GlueServiceHealth) -> Void)?
    private var lastSentQueueOrder: [String] = []
    private var lastSentThumbEdge = defaultThumbEdge
    private var lastQueueItems: [QueueItem] = []
    private var lastState: SpPlayerState?
    private var lastEmittedRemoteVolume: Float?
    private var likedOverride: [String: Bool] = [:]
    private var artHeroEdge = defaultHeroEdge
    private var artThumbEdge = defaultThumbEdge
    private var lastKnownDeviceCount: Int?
    private var wakeOnEmptyCluster = false

    private var emitTask: Task<Void, Never>?
    private var emitContinuation: AsyncStream<EmitJob>.Continuation?

    private enum EmitJob {
        case player(snapshot: BridgethingSchema.PlayerState, hasItem: Bool, onRemote: Bool)
        case queue(entries: [QueueItem], thumbEdge: Int)
    }

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
        workerBase: String,
        psk: String,
        deviceId: String,
        tokenStore: any Spotify.TokenStore,
        clientFactory: SpotifyClientFactory? = nil,
        urlSession: URLSession = SpotifyGlue.defaultImageSession
    ) {
        self.workerBase = workerBase
        self.psk = psk
        self.deviceId = deviceId
        self.tokenStore = tokenStore
        self.urlSession = urlSession
        self.clientFactory = clientFactory ?? { store, observer in
            let client = SpotifyClient.create(base: workerBase, psk: psk, deviceId: deviceId, store: store, observer: observer)
            #if canImport(Darwin)
                #if DEBUG
                    let directive = "spotify=trace"
                #else
                    let directive = "spotify=info"
                #endif
                initLogging(sink: OsLogSink(), directive: directive)
                client.setWsTransport(transport: UrlSessionWsTransport())
                client.setHttpTransport(transport: UrlSessionHttpTransport())
            #endif
            return client
        }
    }

    // MARK: - synchronized handles

    private func currentGateway() -> BridgethingGateway? { stateLock.withLock { gateway } }
    private func setGateway(_ g: BridgethingGateway?) { stateLock.withLock { gateway = g } }

    fileprivate func wakePhoneSpotify() {
        guard let gateway = currentGateway() else { return }
        Task { try? await gateway.player.requestSpotifyWake() }
    }
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
        resetQueueDedup()
        startEmitter()

        let client = clientFactory(tokenStore, ObserverBridge(self))
        if let real = client as? SpotifyClient {
            real.setDeviceWaker(waker: GatewayDeviceWaker(glue: self))
        }
        self.client = client

        currentAuthObserver()?(.pending(nil))
        glueLog.info("attach: spawning connect()")
        connectTask = Task { [weak self, weak client] in
            do {
                try await client?.connect()
            } catch {
                glueLog.error("connect() threw: \(String(describing: error), privacy: .public)")
                self?.currentAuthObserver()?(.failed("sign-in error: \(error)"))
            }
        }
        #if os(iOS)
            foregroundTask = Task { [weak client] in
                for await _ in NotificationCenter.default.notifications(named: UIApplication.didBecomeActiveNotification) {
                    await client?.resync()
                }
            }
        #endif
    }

    public func detach() async {
        stateLock.withLock {
            authObserver = nil
            serviceHealthObserver = nil
        }
        connectTask?.cancel()
        connectTask = nil
        foregroundTask?.cancel()
        foregroundTask = nil
        if let client { await client.disconnect() }
        await releaseAllAuthority()
        let npObs = stateLock.withLock { nowPlayingObserver }
        npObs?(nil)
        resetQueueDedup()
        stopEmitter()
        stateLock.withLock {
            nowPlayingObserver = nil
            lastState = nil
            likedOverride.removeAll()
        }
        client = nil
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
        case let .player(snapshot, hasItem, onRemote):
            if hasItem {
                await claimAuthority([.nowPlayingPlayback, .nowPlayingMetadata], forRemote: onRemote)
                if onRemote { await emitRemoteVolumeFromCluster() }
            } else {
                await releaseAllAuthority()
            }
            try? await gateway.player.snapshot(snapshot)
        case let .queue(entries, thumbEdge):
            await sendQueueChangedIfNeeded(entries, thumbEdge: thumbEdge)
        }
    }

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

    public func handlePeerConnected() async {
        guard currentGateway() != nil else { return }
        let fireWake = stateLock.withLock { () -> Bool in
            if let count = lastKnownDeviceCount { return count == 0 }
            wakeOnEmptyCluster = true
            return false
        }
        if fireWake {
            glueLog.info("connect cluster empty at peer connect; requesting spotify wake")
            wakePhoneSpotify()
        }
        stateLock.withLock { heldScopes.removeAll() }
        resetQueueDedup()
        guard var pending = stateLock.withLock({ lastState }), pending.track != nil else { return }
        if let fresh = await client?.currentPositionMs() { pending.positionMs = fresh }
        enqueue(.player(snapshot: makeSnapshot(from: pending), hasItem: true, onRemote: pending.onRemoteSpeaker))
        let queue = stateLock.withLock { lastQueueItems }
        if !queue.isEmpty {
            enqueue(.queue(entries: queue, thumbEdge: artEdges().thumb))
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

    // MARK: - dealer firehose (Observer)

    fileprivate func onPlayer(_ state: SpPlayerState) {
        guard currentGateway() != nil else { return }
        let heroEdge = artEdges().hero
        let (liked, likeSupported) = likeFields(for: state.track)
        currentNowPlayingObserver()?(GlueNowPlaying(
            update: Self.makeUpdate(from: state, heroEdge: heroEdge, liked: liked, likeSupported: likeSupported),
            artworkUrl: state.track.flatMap { Self.rawArtworkURL(Self.bestHex($0)) }
        ))
        stateLock.withLock { lastState = state }
        let snapshot = makeSnapshot(from: state, heroEdge: heroEdge, liked: liked, likeSupported: likeSupported)
        let hasItem = state.track != nil
        let onRemote = state.onRemoteSpeaker
        enqueue(.player(snapshot: snapshot, hasItem: hasItem, onRemote: onRemote))
    }

    fileprivate func onQueue(_ queue: SpQueue) {
        let thumb = artEdges().thumb
        let entries = Array(queue.next.prefix(queueMax).map { Self.queueItem(from: $0, maxEdge: thumb) })
        stateLock.withLock { lastQueueItems = entries }
        enqueue(.queue(entries: entries, thumbEdge: thumb))
    }

    fileprivate func onDevices(_ devices: [SpDevice]) {
        let fireWake = stateLock.withLock { () -> Bool in
            lastKnownDeviceCount = devices.count
            guard wakeOnEmptyCluster else { return false }
            wakeOnEmptyCluster = false
            return devices.isEmpty
        }
        if fireWake {
            glueLog.info("connect cluster empty at peer connect; requesting spotify wake")
            wakePhoneSpotify()
        }
    }

    fileprivate func onLibraryChanged(_ scope: SpLibraryScope) {
        guard let gateway = currentGateway() else { return }
        let wireScope: BridgethingSchema.LibraryScope = switch scope {
        case .saved: .saved
        case .playlists: .playlists
        }
        Task { try? await gateway.library.libraryChanged(LibraryChanged(scope: wireScope)) }
    }

    fileprivate func onAuth(_ state: SpAuthState) {
        glueLog.info("auth state: \(String(describing: state), privacy: .public)")
        switch state {
        case let .loggedIn(username):
            _ = username
            currentAuthObserver()?(.authenticated)
            Task { [weak self] in await self?.checkPremium() }
        case .loggedOut:
            handleAuthDown()
            currentAuthObserver()?(.pending(nil))
        case let .pending(url, code):
            let u = URL(string: url) ?? URL(string: "https://spotify.com")!
            currentAuthObserver()?(.pending(GlueDeviceCodePrompt(userCode: code, verificationURL: u, verificationURLComplete: u)))
        case let .failed(reason):
            handleAuthDown()
            currentAuthObserver()?(.failed(reason))
        }
    }

    private func handleAuthDown() {
        currentNowPlayingObserver()?(nil)
        let held = stateLock.withLock { !heldScopes.isEmpty }
        if currentGateway() != nil, held { Task { await releaseAllAuthority() } }
    }

    private func checkPremium() async {
        guard let client else { return }
        guard let product = try? await client.product() else { return }
        if !product.canUseSuperbird {
            currentAuthObserver()?(.failed("Spotify Premium is required"))
        }
    }

    // MARK: - inbound transport

    public func play(_ uri: PlayUri) async throws {
        guard let client else { throw GlueError.detached }
        if let ctx = uri.context {
            try await client.play(uri: ctx.contextUri, skipToUri: uri.uri)
        } else {
            try await client.play(uri: uri.uri, skipToUri: nil)
        }
    }

    public func queue(_ req: QueueUri) async throws {
        guard let client else { throw GlueError.detached }
        if case .index = req.position { throw GlueError.notImplemented }
        try await client.queueUri(uri: req.uri)
    }

    public func pause() async throws { try await require().pause() }
    public func resume() async throws { try await require().resume() }
    public func skipNext() async throws { try await require().skipNext() }
    public func skipPrev() async throws { try await require().skipPrev() }

    public func skipToIndex(_ index: UInt32) async throws {
        let client = try require()
        let (items, contextUri) = stateLock.withLock { (lastQueueItems, lastState?.contextUri ?? "") }
        guard index < UInt32(items.count), !contextUri.isEmpty else {
            throw GlueError.underlying(SpotifyGlueError.queueIndexOutOfRange(index: index, count: items.count))
        }
        try await client.play(uri: contextUri, skipToUri: items[Int(index)].uri)
    }
    public func seekTo(_ ms: UInt32) async throws { try await require().seek(positionMs: Int64(ms)) }
    public func setShuffle(_ on: Bool) async throws { try await require().setShuffle(on: on) }

    public func setRepeat(_ mode: WireRepeat) async throws {
        let mapped: Spotify.RepeatMode = switch mode {
        case .off: .off
        case .all: .context
        case .one: .track
        }
        try await require().setRepeat(mode: mapped)
    }

    private func require() throws -> any SpotifyClientProviding {
        guard let client else { throw GlueError.detached }
        return client
    }

    // MARK: - library

    public func search(_ req: LibrarySearchRequest) async throws -> SearchResult {
        let client = try require()
        let kinds = (req.kinds?.isEmpty == false) ? req.kinds! : [.track, .album, .artist, .playlist]
        let res = try await client.search(query: req.query, limit: req.limit)
        let edge = artEdges().hero
        let limit = Int(req.limit)
        var items: [LibraryItem] = []
        var present: [ItemKind] = []
        var full = false
        for kind in kinds {
            let arr: [SpBrowseItem]
            switch kind {
            case .track: arr = res.tracks
            case .album: arr = res.albums
            case .artist: arr = res.artists
            case .playlist: arr = res.playlists
            default: arr = []
            }
            let mapped = arr.compactMap { Self.libraryItem($0, edge: edge) }
            if !mapped.isEmpty {
                present.append(kind)
                if mapped.count >= limit { full = true }
            }
            items.append(contentsOf: mapped)
        }
        return SearchResult(items: items, kinds: present, total: nil, hasMore: full)
    }

    public func browse(_ req: LibraryBrowseRequest) async throws -> BrowseResult {
        let client = try require()
        let edge = artEdges().hero
        let result: BrowseResult
        switch req.nodeId {
        case nil, "", "root":
            let shelves = try await client.rootBrowse()
            result = BrowseResult(
                entries: shelves.map { .folder(Self.folder($0, edge: edge)) },
                total: UInt32(shelves.count), hasMore: false
            )
        default:
            let page = try await client.browse(nodeId: req.nodeId!, limit: req.limit, offset: req.offset)
            let entries = page.items.compactMap { Self.libraryItem($0, edge: edge).map { BrowseEntry.item($0) } }
            result = BrowseResult(entries: entries, total: page.total, hasMore: page.hasMore)
        }
        warmArt(in: result)
        return result
    }

    public func resolveContext(_ uri: String) async throws -> ContextResolveReply {
        let client = try require()
        let b = try await client.resolveContext(uri: uri)
        return ContextResolveReply(
            name: b.title.isEmpty ? nil : b.title,
            artworkId: Self.artAssetId(b.imageId, edge: artEdges().hero),
            subtitle: b.subtitle.isEmpty ? nil : b.subtitle
        )
    }

    public func recommendations(_ req: LibraryRecommendationsRequest) async throws -> RecommendationsResult {
        let client = try require()
        let edge = artEdges().hero
        if let artist = req.seeds.first(where: { $0.kind == .artist }) {
            let page = try await client.browse(nodeId: artist.uri, limit: req.limit, offset: 0)
            return RecommendationsResult(items: page.items.compactMap { Self.libraryItem($0, edge: edge) }, total: nil, hasMore: false)
        }
        return RecommendationsResult(items: [], total: nil, hasMore: false)
    }

    public func favoritesList(_ req: LibraryFavoritesListRequest) async throws -> FavoritesPage {
        let client = try require()
        let edge = artEdges().hero
        let page = try await client.favoritesList(limit: req.limit, offset: req.offset)
        return FavoritesPage(items: page.items.compactMap { Self.libraryItem($0, edge: edge) }, total: page.total, hasMore: page.hasMore)
    }

    public func favoritesContains(_ req: LibraryFavoritesContainsRequest) async throws -> [Bool] {
        try await require().favoritesContains(uris: req.uris)
    }

    public func favoritesToggle(_ item: ItemRef) async throws {
        let client = try require()
        let saved = (try await client.favoritesContains(uris: [item.uri])).first ?? false
        try await client.favoritesSet(uri: item.uri, liked: !saved)
        await applyLikedChange(uri: item.uri, liked: !saved)
    }

    public func favoritesSet(_ item: ItemRef, liked: Bool) async throws {
        try await require().favoritesSet(uri: item.uri, liked: liked)
        await applyLikedChange(uri: item.uri, liked: liked)
    }

    public func favoritesSetMany(_ entries: [FavoritesSet]) async throws {
        let client = try require()
        for entry in entries {
            try await client.favoritesSet(uri: entry.item.uri, liked: entry.liked)
            await applyLikedChange(uri: entry.item.uri, liked: entry.liked)
        }
    }

    // MARK: - assets

    public func asset(id: String) async throws -> AssetBytes? {
        guard let parsed = Self.parseImageId(id) else { return nil }
        let (data, _) = try await urlSession.data(from: parsed.url)
        return autoreleasepool {
            Self.downsample(data, maxEdge: parsed.maxEdge).map { AssetBytes(bytes: $0, mime: "image/jpeg") }
        }
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

    // MARK: - art profile

    public func setArtProfile(heroPx: Int, thumbPx: Int) {
        stateLock.withLock {
            artHeroEdge = max(1, heroPx)
            artThumbEdge = max(1, thumbPx)
        }
    }

    private func artEdges() -> (hero: Int, thumb: Int) {
        stateLock.withLock { (artHeroEdge, artThumbEdge) }
    }

    // MARK: - outbound snapshot / queue

    private func makeSnapshot(from state: SpPlayerState) -> BridgethingSchema.PlayerState {
        let (liked, supported) = likeFields(for: state.track)
        return makeSnapshot(from: state, heroEdge: artEdges().hero, liked: liked, likeSupported: supported)
    }

    private func makeSnapshot(from state: SpPlayerState, heroEdge: Int, liked: Bool?, likeSupported: Bool?) -> BridgethingSchema.PlayerState {
        let track: MediaItem? = state.track.map { t in
            MediaItem(
                uri: t.uri,
                persistentId: t.uri,
                title: t.name.isEmpty ? nil : t.name,
                album: t.album.name.isEmpty ? nil : t.album.name,
                albumUri: t.album.uri.isEmpty ? nil : t.album.uri,
                albumArtist: nil,
                artist: Self.artistNames(t),
                artistUri: t.artists.first?.uri,
                liked: liked,
                artworkId: Self.artAssetId(Self.bestHex(t), edge: heroEdge),
                durationMs: t.durationMs,
                mediaTypes: nil,
                trackNumber: nil,
                trackCount: nil,
                isLikeSupported: likeSupported,
                isBanSupported: nil,
                isBanned: nil,
                chapterCount: nil
            )
        }
        let playback = Playback(
            state: state.isPaused ? .paused : .playing,
            positionMs: state.positionMs,
            shuffle: state.shuffle,
            shuffleMode: state.shuffle ? .songs : .off,
            repeat: Self.mapRepeat(state.repeat),
            queueIndex: nil,
            queueCount: nil,
            queueChapterIndex: nil,
            setElapsedTimeAvailable: state.canSeek,
            queueListAvail: nil,
            appleMusicRadioAd: nil
        )
        let context: PlaybackContext? = state.contextUri.isEmpty ? nil :
            PlaybackContext(uri: state.contextUri, name: state.contextName.isEmpty ? nil : state.contextName)
        return BridgethingSchema.PlayerState(
            track: track, playback: playback, queue: [],
            options: PlayerOptions(speed: 1.0, crossfade_ms: nil), context: context
        )
    }

    private func sendQueueChangedIfNeeded(_ entries: [QueueItem], thumbEdge: Int) async {
        guard let gateway = currentGateway() else { return }
        let order = entries.map(\.uri)
        let edgeChanged = stateLock.withLock { () -> Bool in
            let changed = thumbEdge != lastSentThumbEdge
            lastSentThumbEdge = thumbEdge
            return changed
        }
        let lastOrder = stateLock.withLock { lastSentQueueOrder }
        if !edgeChanged, let runway = Self.forwardSlideRunway(from: lastOrder, to: order), runway >= queueRunwayFloor {
            return
        }
        do {
            try await gateway.player.queueChanged(QueueSnapshot(order: order, items: entries))
            stateLock.withLock { lastSentQueueOrder = order }
        } catch {
            // leave last-sent state unchanged so the next change re-sends.
        }
    }

    private func resetQueueDedup() {
        stateLock.withLock { lastSentQueueOrder = [] }
    }

    private static func forwardSlideRunway(from last: [String], to new: [String]) -> Int? {
        guard !last.isEmpty else { return nil }
        for k in 1 ..< last.count {
            let suffix = Array(last[k...])
            if new.count >= suffix.count, Array(new.prefix(suffix.count)) == suffix {
                return suffix.count
            }
        }
        return nil
    }

    // MARK: - liked

    private func likeFields(for track: SpTrack?) -> (liked: Bool?, supported: Bool?) {
        guard let track, Self.isSpotifyUri(track.uri) else { return (nil, nil) }
        let liked: Bool = stateLock.withLock {
            guard let override = likedOverride[track.uri] else { return track.saved }
            if override == track.saved { likedOverride[track.uri] = nil }
            return override
        }
        return (liked, true)
    }

    private func applyLikedChange(uri: String, liked: Bool) async {
        stateLock.withLock { likedOverride[uri] = liked }
        await reemitSnapshotIfCurrent(uri: uri)
    }

    private func reemitSnapshotIfCurrent(uri: String) async {
        let pending = stateLock.withLock { lastState }
        guard let pending, pending.track?.uri == uri, let gateway = currentGateway() else { return }
        try? await gateway.player.snapshot(makeSnapshot(from: pending))
    }

    // MARK: - authority

    private func claimAuthority(_ scopes: Set<CompanionAuthorityScope>, forRemote onRemote: Bool) async {
        guard let gateway = currentGateway() else { return }
        var want = scopes
        if onRemote { want.insert(.volume) }
        for scope in want {
            let needs = stateLock.withLock { () -> Bool in
                if heldScopes.contains(scope) { return false }
                heldScopes.insert(scope)
                return true
            }
            if needs {
                try? await gateway.authority.claim(AuthorityClaim(scope: scope, appBundle: spotifyAppBundle))
                if scope == .volume { await emitRemoteVolumeFromCluster(force: true) }
            }
        }
        if !onRemote {
            let dropVolume = stateLock.withLock { () -> Bool in
                lastEmittedRemoteVolume = nil
                return heldScopes.remove(.volume) != nil
            }
            if dropVolume { try? await gateway.authority.release(AuthorityRelease(scope: .volume)) }
        }
    }

    private func releaseAllAuthority() async {
        guard let gateway = currentGateway() else { return }
        let scopes = stateLock.withLock { () -> [CompanionAuthorityScope] in
            let s = Array(heldScopes)
            heldScopes.removeAll()
            lastEmittedRemoteVolume = nil
            return s
        }
        for scope in scopes { try? await gateway.authority.release(AuthorityRelease(scope: scope)) }
    }

    // MARK: - remote connect-device volume

    private static let volumeStepPercent = 6.25

    public func ownsVolume() async -> Bool {
        stateLock.withLock { heldScopes.contains(.volume) }
    }

    public func volumeUp() async throws {
        let target = try await require().volumeStep(deltaPercent: Self.volumeStepPercent)
        await emitRemoteVolume(level: Float(target / 100.0))
    }

    public func volumeDown() async throws {
        let target = try await require().volumeStep(deltaPercent: -Self.volumeStepPercent)
        await emitRemoteVolume(level: Float(target / 100.0))
    }

    public func setVolume(_ level: Float) async throws {
        try await require().setVolume(percent: Double(level) * 100.0)
        await emitRemoteVolume(level: level)
    }

    private func emitRemoteVolumeFromCluster(force: Bool = false) async {
        guard let pct = await client?.activeDeviceVolumePercent() else { return }
        await emitRemoteVolume(level: Float(pct / 100.0), force: force)
    }

    private func emitRemoteVolume(level: Float, force: Bool = false) async {
        guard let gateway = currentGateway() else { return }
        let changed = stateLock.withLock { () -> Bool in
            guard heldScopes.contains(.volume) else { return false }
            if !force, let last = lastEmittedRemoteVolume, abs(last - level) < 0.005 { return false }
            lastEmittedRemoteVolume = level
            return true
        }
        if changed { try? await gateway.audio.volumeChanged(VolumeChanged(level: level, muted: false)) }
    }

    // MARK: - reduced -> wire mapping

    private static func artistNames(_ t: SpTrack) -> String? {
        let s = t.artists.map(\.name).joined(separator: ", ")
        return s.isEmpty ? nil : s
    }

    private static func bestHex(_ t: SpTrack) -> String {
        t.imageId.isEmpty ? t.album.imageId : t.imageId
    }

    private static func artAssetId(_ ref: String, edge: Int) -> String? {
        guard !ref.isEmpty else { return nil }
        return imageAssetId(ref.hasPrefix("http") ? ref : "\(scdnImagePrefix)\(ref)", maxEdge: edge)
    }

    private static func rawArtworkURL(_ ref: String) -> String? {
        guard !ref.isEmpty else { return nil }
        return ref.hasPrefix("http") ? ref : "\(scdnImagePrefix)\(ref)"
    }

    private static func mapRepeat(_ mode: Spotify.RepeatMode) -> WireRepeat {
        switch mode {
        case .off: .off
        case .context: .all
        case .track: .one
        }
    }

    private static func isSpotifyUri(_ uri: String) -> Bool {
        uri.hasPrefix("spotify:")
    }

    private static func kind(of uri: String) -> String {
        let parts = uri.split(separator: ":")
        return parts.count >= 2 ? String(parts[1]) : ""
    }

    private static func mapTrack(_ b: SpBrowseItem, edge: Int) -> BridgethingSchema.Track {
        BridgethingSchema.Track(
            id: b.uri,
            name: b.title,
            album: BridgethingSchema.Album(id: b.album.uri, name: b.album.name, artwork_id: nil),
            artist: BridgethingSchema.Artist(id: b.artists.first?.uri ?? "", name: b.artists.first?.name ?? "", artwork_id: nil),
            artists: b.artists.map { BridgethingSchema.Artist(id: $0.uri, name: $0.name, artwork_id: nil) },
            duration_ms: b.durationMs,
            image_id: artAssetId(b.imageId, edge: edge) ?? "",
            saved: b.saved
        )
    }

    private static func libraryItem(_ b: SpBrowseItem, edge: Int) -> LibraryItem? {
        let art = artAssetId(b.imageId, edge: edge)
        switch kind(of: b.uri) {
        case "track":
            return .track(mapTrack(b, edge: edge))
        case "album":
            return .album(BridgethingSchema.Album(id: b.uri, name: b.title, artwork_id: art))
        case "artist":
            return .artist(BridgethingSchema.Artist(id: b.uri, name: b.title, artwork_id: art))
        case "playlist":
            return .playlist(Playlist(uri: b.uri, name: b.title, ownerName: nil, trackCount: nil, artworkId: art))
        case "user" where b.uri.hasSuffix(":collection"):
            return .playlist(Playlist(uri: b.uri, name: b.title, ownerName: nil, trackCount: nil, artworkId: art))
        case "show":
            return .show(Show(uri: b.uri, name: b.title, publisher: b.subtitle.isEmpty ? nil : b.subtitle, episodeCount: nil, artworkId: art))
        case "episode":
            return .podcastEpisode(PodcastEpisode(uri: b.uri, name: b.title, showName: b.subtitle.isEmpty ? nil : b.subtitle, durationMs: b.durationMs, publishedAtUnixS: nil, artworkId: art))
        case "station":
            return .station(Station(uri: b.uri, name: b.title, seed: nil, artworkId: art))
        default:
            return nil
        }
    }

    private static func folder(_ s: SpShelf, edge: Int) -> BrowseFolder {
        let children = s.items.compactMap { libraryItem($0, edge: edge).map { BrowseEntry.item($0) } }
        return BrowseFolder(
            nodeId: s.id, title: s.title, subtitle: nil, artworkId: nil,
            total: UInt32(s.items.count), previewChildren: children
        )
    }

    private static func queueItem(from t: SpTrack, maxEdge: Int) -> QueueItem {
        QueueItem(
            uri: t.uri,
            title: t.name.isEmpty ? nil : t.name,
            artist: artistNames(t),
            artistUri: t.artists.first?.uri,
            album: t.album.name.isEmpty ? nil : t.album.name,
            albumUri: t.album.uri.isEmpty ? nil : t.album.uri,
            artworkId: artAssetId(bestHex(t), edge: maxEdge),
            durationMs: t.durationMs,
            persistentId: nil,
            queued: t.queued
        )
    }

    private static func makeUpdate(from state: SpPlayerState, heroEdge: Int, liked: Bool?, likeSupported: Bool?) -> NowPlayingUpdate {
        let media: MediaItemUpdate? = state.track.map { t in
            MediaItemUpdate(
                persistentId: t.uri,
                title: t.name.isEmpty ? nil : t.name,
                album: t.album.name.isEmpty ? nil : t.album.name,
                albumUri: t.album.uri.isEmpty ? nil : t.album.uri,
                albumArtist: nil,
                artist: artistNames(t),
                artistUri: t.artists.first?.uri,
                liked: liked,
                artworkId: artAssetId(bestHex(t), edge: heroEdge),
                durationMs: t.durationMs,
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
        let playback = PlaybackUpdate(
            playing: !state.isPaused,
            positionMs: state.positionMs,
            shuffle: state.shuffle,
            shuffleMode: state.shuffle ? .songs : .off,
            repeat: mapRepeat(state.repeat),
            appBundle: spotifyAppBundle,
            appDisplayName: "Spotify",
            queueIndex: nil,
            queueCount: nil,
            queueChapterIndex: nil,
            playbackSpeed: nil,
            setElapsedTimeAvailable: state.canSeek,
            queueListAvail: nil,
            appleMusicRadioAd: nil,
            appleMusicRadioStationName: nil
        )
        return NowPlayingUpdate(mediaItem: media, playback: playback)
    }

    // MARK: - art asset id codec + downsample

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
        guard let slash = rest.firstIndex(of: "/"), let maxEdge = Int(rest[..<slash]) else { return nil }
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
            guard let dest = CGImageDestinationCreateWithData(out as CFMutableData, "public.jpeg" as CFString, 1, nil) else { return nil }
            CGImageDestinationAddImage(dest, thumb, [kCGImageDestinationLossyCompressionQuality: 0.6] as CFDictionary)
            guard CGImageDestinationFinalize(dest) else { return nil }
            return out as Data
        #else
            return nil
        #endif
    }
}

private final class ObserverBridge: Spotify.Observer, @unchecked Sendable {
    private weak var glue: SpotifyGlue?
    init(_ glue: SpotifyGlue) { self.glue = glue }
    func onPlayer(state: SpPlayerState) { glue?.onPlayer(state) }
    func onQueue(queue: SpQueue) { glue?.onQueue(queue) }
    func onDevices(devices: [SpDevice]) { glue?.onDevices(devices) }
    func onAuth(state: SpAuthState) { glue?.onAuth(state) }
    func onLibraryChanged(scope: SpLibraryScope) { glue?.onLibraryChanged(scope) }
}

private enum SpotifyGlueError: Swift.Error, CustomStringConvertible, LocalizedError {
    case queueIndexOutOfRange(index: UInt32, count: Int)

    var description: String {
        switch self {
        case let .queueIndexOutOfRange(index, count): "queue index \(index) out of range (\(count) upcoming)"
        }
    }

    var errorDescription: String? { description }
}

private extension LibraryItem {
    var artworkId: String? {
        switch self {
        case let .track(t): return t.image_id.isEmpty ? nil : t.image_id
        case let .playlist(p): return p.artworkId
        case let .podcastEpisode(e): return e.artworkId
        case let .show(s): return s.artworkId
        case let .station(s): return s.artworkId
        case let .album(a): return a.artwork_id
        case let .artist(a): return a.artwork_id
        }
    }
}

final class GatewayDeviceWaker: Spotify.DeviceWaker, @unchecked Sendable {
    private weak var glue: SpotifyGlue?

    init(glue: SpotifyGlue) {
        self.glue = glue
    }

    func wakeDevice() {
        glue?.wakePhoneSpotify()
    }
}
