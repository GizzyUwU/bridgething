import BridgethingGateway
import BridgethingGlue
import BridgethingLyrics
import BridgethingSchema
import Foundation
import os

private let osLog = Logger(subsystem: "dev.bridgething.companion", category: "core")

/// Severity tag passed to the `BridgethingCompanion` log observer.
public enum CompanionLogLevel: String, Sendable {
    case debug, info, warn, error
}

/// Version stamp the companion announces in `GatewayInfo`. Hardcoded to
/// match the Rust workspace `version` and bumped at release time.
public enum BridgethingCompanionVersion {
    public static let lib: String = "0.1.0"
    public static let libbridgething: String = "0.1.0"
}

/// Identity the companion advertises in `GatewayCapabilities.gateway`.
/// Caller-supplied at companion init. The `address` is whatever stable
/// identifier the host platform exposes for itself (iOS:
/// `UIDevice.identifierForVendor`, Android: `Settings.Secure.ANDROID_ID`).
/// Empty string is acceptable when no stable identifier is available.
public struct HostInfo: Sendable {
    public let appName: String
    public let appVersion: String
    public let osName: String
    public let osVersion: String
    public let address: String
    public let adapterVersion: String

    public init(
        appName: String,
        appVersion: String,
        osName: String,
        osVersion: String = "",
        address: String = "",
        adapterVersion: String = ""
    ) {
        self.appName = appName
        self.appVersion = appVersion
        self.osName = osName
        self.osVersion = osVersion
        self.address = address
        self.adapterVersion = adapterVersion
    }
}

/// Capability flags the companion declares. Glue contributions
/// (`uriSchemes`, `musicProvider`, `lyricsSupported`) are mixed in by
/// `BridgethingCompanion` at announce time.
public struct CompanionCapabilityFlags: Sendable {
    public var geo: Bool
    public var notifications: Bool
    public var netFetch: Bool
    public var netWs: Bool
    public var audioTts: Bool

    public init(
        geo: Bool = true,
        notifications: Bool = false,
        netFetch: Bool = true,
        netWs: Bool = true,
        audioTts: Bool = false
    ) {
        self.geo = geo
        self.notifications = notifications
        self.netFetch = netFetch
        self.netWs = netWs
        self.audioTts = audioTts
    }
}

/// Top-level orchestrator for the bridgething companion app.
///
/// Owns one `BridgethingGateway` over the supplied transport adapter.
/// Holds at most one active `BridgethingGlue`. Runs every companion-side
/// dispatcher as long-lived child tasks while started: Player verbs to
/// glue, Lyrics requests with resolver fallback, Asset requests to glue,
/// Net (fetch/ws/stream) via URLSession, Geo via CoreLocation, Volume
/// via AVAudioSession.
public actor BridgethingCompanion {
    public nonisolated let gateway: BridgethingGateway

    private let host: HostInfo
    private let lyricsResolver: any LyricsResolver
    private var capFlags: CompanionCapabilityFlags

    private var activeGlue: (any BridgethingGlue)?
    private var tasks: [Task<Void, Never>] = []
    private var started = false
    private var nowPlayingObserver: (@Sendable (GlueNowPlaying?) -> Void)?
    private var ancsAuthStateObserver: (@Sendable (AncsAuthState) -> Void)?
    private var ancsAuthState: AncsAuthState = .unknown
    private var logObserver: (@Sendable (CompanionLogLevel, String) -> Void)?

    private let netDispatcher: NetDispatcher
    public let ota: OtaService
    #if canImport(CoreLocation)
        private let geoController: GeoController
    #endif
    #if os(iOS)
        private let volumeMonitor: VolumeMonitor
    #endif

    public init(
        adapter: any Adapter,
        lyricsResolver: any LyricsResolver,
        host: HostInfo,
        capabilities: CompanionCapabilityFlags = CompanionCapabilityFlags(),
        geoProvider: (any GeoLocationProviding)? = nil
    ) {
        self.host = host
        self.lyricsResolver = lyricsResolver
        capFlags = capabilities
        gateway = BridgethingGateway(adapter: adapter)
        netDispatcher = NetDispatcher()
        ota = OtaService()
        #if canImport(CoreLocation)
            geoController = GeoController(provider: geoProvider)
        #endif
        #if os(iOS)
            volumeMonitor = VolumeMonitor()
        #endif
    }

    public func start() async throws {
        if started { return }
        try await gateway.start()
        started = true
        log(.info, "companion started")

        spawnDispatchers()

        #if os(iOS)
            await volumeMonitor.start { [weak self] level, muted in
                Task { await self?.broadcastVolume(level: level, muted: muted) }
            }
            if let snapshot = await volumeMonitor.snapshot() {
                await broadcastVolume(level: snapshot.level, muted: snapshot.muted)
                try? await gateway.authority.claim(AuthorityClaim(scope: .volume))
            }
        #endif
    }

    public func stop() async {
        for task in tasks {
            task.cancel()
        }
        tasks.removeAll()

        #if os(iOS)
            await volumeMonitor.stop()
            try? await gateway.authority.release(AuthorityRelease(scope: .volume))
        #endif
        #if canImport(CoreLocation)
            await geoController.stop()
        #endif
        await netDispatcher.stop()
        await ota.stop()

        if let glue = activeGlue {
            await glue.detach()
        }
        activeGlue = nil

        await gateway.stop()
        started = false
        log(.info, "companion stopped")
    }

    /// Actor-isolated convenience: capture the current observer + emit.
    private func log(_ level: CompanionLogLevel, _ message: String) {
        emitLog(level, message, observer: logObserver)
    }

    public func setActive(_ glue: (any BridgethingGlue)?) async throws {
        if let activeGlue {
            log(.info, "detaching glue \(type(of: activeGlue).name)")
            await activeGlue.detach()
            nowPlayingObserver?(nil)
        }
        activeGlue = glue
        if let glue {
            if let observer = nowPlayingObserver {
                await glue.setNowPlayingObserver(observer)
            }
            do {
                try await glue.attach(gateway: gateway)
                log(.info, "attached glue \(type(of: glue).name)")
            } catch {
                log(.error, "glue \(type(of: glue).name) attach failed: \(error.localizedDescription)")
                throw error
            }
        }
        await announceCapabilities()
    }

    public func current() -> (any BridgethingGlue)? {
        activeGlue
    }

    /// Subscribe to NowPlaying mirror updates from whichever glue is
    /// active. Replacing the observer takes effect immediately for the
    /// active glue and persists across `setActive` swaps.
    public func setNowPlayingObserver(_ observer: (@Sendable (GlueNowPlaying?) -> Void)?) async {
        nowPlayingObserver = observer
        if let glue = activeGlue {
            await glue.setNowPlayingObserver(observer ?? { _ in })
        }
    }

    /// Subscribe to ANCS authorization-state transitions reported by the
    /// daemon. iOS-only signal; on Android the observer never fires.
    public func setAncsAuthStateObserver(_ observer: (@Sendable (AncsAuthState) -> Void)?) {
        ancsAuthStateObserver = observer
    }

    /// Subscribe to companion-side log events. Replaces / un-installs
    /// the previous observer. Companion still emits to `os.Logger`
    /// regardless; the observer is for surfacing to a host UI (e.g. the
    /// RN debug-log screen).
    public func setLogObserver(_ observer: (@Sendable (CompanionLogLevel, String) -> Void)?) {
        logObserver = observer
    }

    nonisolated func emitLog(_ level: CompanionLogLevel, _ message: String, observer: (@Sendable (CompanionLogLevel, String) -> Void)?) {
        switch level {
        case .debug: osLog.debug("\(message, privacy: .public)")
        case .info: osLog.info("\(message, privacy: .public)")
        case .warn: osLog.warning("\(message, privacy: .public)")
        case .error: osLog.error("\(message, privacy: .public)")
        }
        observer?(level, message)
    }

    /// Last daemon-reported ANCS auth state. `unknown` until the daemon
    /// emits one (no iAP2 link yet, or session task hasn't probed).
    public func currentAncsAuthState() -> AncsAuthState {
        ancsAuthState
    }

    /// Drive the AccessorySetupKit pair flow that creates the LE bond
    /// the daemon needs before iOS will expose ANCS. iOS 18+ only;
    /// returns `unsupported` on Android / earlier iOS.
    public func enableAncsNotifications() async -> AncsSetupResult {
        #if os(iOS)
            let coordinator = await makeOrReuseCoordinator()
            await coordinator.setLastAuthState(ancsAuthState)
            return await coordinator.pair()
        #else
            return AncsSetupResult(kind: .unsupported, authState: ancsAuthState)
        #endif
    }

    #if os(iOS)
        private var ancsCoordinator: AncsPairCoordinator?
        private var pickerCoordinator: AccessoryPickerCoordinator?

        private func makeOrReuseCoordinator() async -> AncsPairCoordinator {
            if let existing = ancsCoordinator { return existing }
            let coordinator = await MainActor.run { AncsPairCoordinator() }
            ancsCoordinator = coordinator
            return coordinator
        }

        private func makeOrReusePicker() async -> AccessoryPickerCoordinator {
            if let existing = pickerCoordinator { return existing }
            let coordinator = await MainActor.run { AccessoryPickerCoordinator() }
            pickerCoordinator = coordinator
            return coordinator
        }
    #endif

    /// Present AccessorySetupKit's system picker (iOS 18+) and return
    /// the chosen accessory, or `nil` on cancel. iOS <=17 and non-iOS
    /// platforms return `nil` immediately - callers branch on
    /// `Platform.OS` and use the in-app picker on android.
    public func presentPairPicker() async -> AccessoryPickResult? {
        #if os(iOS)
            if #available(iOS 18.0, *) {
                return await makeOrReusePicker().pick()
            }
            return nil
        #else
            return nil
        #endif
    }

    public func setCapabilityFlags(_ flags: CompanionCapabilityFlags) async {
        capFlags = flags
        await announceCapabilities()
    }

    // MARK: - capability composition

    private func announceCapabilities() async {
        let caps = composeCapabilities()
        try? await gateway.capabilities.announce(caps)
    }

    private func composeCapabilities() -> GatewayCapabilities {
        let glue = activeGlue
        let info = GatewayInfo(
            address: host.address,
            name: host.appName,
            osName: host.osName,
            appName: host.appName,
            appVersion: host.appVersion,
            adapterVersion: host.adapterVersion,
            libVersion: BridgethingCompanionVersion.lib,
            libbridgethingVersion: BridgethingCompanionVersion.libbridgething
        )
        let avail = SurfaceAvailability(
            geo: capFlags.geo,
            notifications: capFlags.notifications,
            netFetch: capFlags.netFetch,
            netWs: capFlags.netWs,
            audioTts: capFlags.audioTts,
            lyrics: true
        )
        return GatewayCapabilities(
            gateway: info,
            uriSchemes: glue?.uriSchemes ?? [],
            network: NetworkInfo(kind: .unknown, metered: false),
            available: avail,
            audio: AudioCapabilities(earcons: [], voices: []),
            musicProvider: glue?.musicProvider ?? .none
        )
    }

    // MARK: - dispatchers

    private func spawnDispatchers() {
        tasks.append(Task { [weak self] in await self?.runConnectAnnouncer() })
        tasks.append(Task { [weak self] in await self?.runPlayerDispatch() })
        tasks.append(Task { [weak self] in await self?.runAssetDispatch() })
        tasks.append(Task { [weak self] in await self?.runLibraryDispatch() })
        tasks.append(Task { [weak self] in await self?.runLyricsDispatch() })
        tasks.append(Task { [weak self] in await self?.runAncsAuthDispatch() })
        tasks.append(Task { [weak self] in
            guard let self else { return }
            await netDispatcher.start(gateway: gateway)
        })
        tasks.append(Task { [weak self] in
            guard let self else { return }
            await ota.start(gateway: gateway)
        })
        #if canImport(CoreLocation)
            tasks.append(Task { [weak self] in
                guard let self else { return }
                await geoController.start(gateway: gateway)
            })
        #endif
    }

    private func runConnectAnnouncer() async {
        for await event in gateway.events {
            switch event {
            case let .connected(device):
                log(.info, "peer connected: \(device.name) [\(device.id)]")
                await announceCapabilities()
            case let .disconnected(id):
                log(.info, "peer disconnected: \(id)")
            case let .decodeError(id, description):
                log(.warn, "[\(id)] decode error: \(description)")
            case .message:
                continue
            }
        }
    }

    private func runPlayerDispatch() async {
        for await event in gateway.events {
            guard case let .message(_, msg) = event,
                  case let .player(player) = msg.data
            else { continue }
            let glue = activeGlue
            guard let glue else { continue }
            let observer = logObserver
            await dispatchPlayer(player, to: glue, logObserver: observer)
        }
    }

    private nonisolated func dispatchPlayer(
        _ player: BridgeToGatewayPlayerMsg,
        to glue: any BridgethingGlue,
        logObserver: (@Sendable (CompanionLogLevel, String) -> Void)?
    ) async {
        do {
            switch player {
            case let .play(p): try await glue.play(p)
            case let .queue(q): try await glue.queue(q)
            case .pause: try await glue.pause()
            case .resume: try await glue.resume()
            case .skipNext: try await glue.skipNext()
            case .skipPrev: try await glue.skipPrev()
            case let .skipToIndex(s): try await glue.skipToIndex(s.index)
            case let .seekTo(s): try await glue.seekTo(s.positionMs)
            case let .setShuffle(s): try await glue.setShuffle(s.on)
            case let .setRepeat(r): try await glue.setRepeat(r.mode)
            case let .setSpeed(s): try await glue.setSpeed(s.speed)
            case let .setCrossfade(s): try await glue.setCrossfade(s.durationMs)
            case let .hint(h): await glue.handlePlaybackHint(h)
            }
        } catch {
            emitLog(
                .warn,
                "player verb \(String(describing: player)) failed: \(error.localizedDescription)",
                observer: logObserver
            )
        }
    }

    private func runAssetDispatch() async {
        for await (handle, req) in gateway.asset.requestRequests {
            await handleAsset(handle: handle, id: req.id)
        }
    }

    private func handleAsset(handle: AssetRequestHandle, id: String) async {
        let bytes: AssetBytes?
        do {
            bytes = try await activeGlue?.asset(id: id)
        } catch {
            log(.warn, "asset \(id) glue resolve failed: \(error.localizedDescription)")
            try? await handle.respondErr(AssetNotFoundReply(id: id))
            return
        }
        guard let bytes else {
            try? await handle.respondErr(AssetNotFoundReply(id: id))
            return
        }
        try? await handle.respond(AssetGotReply(id: id, bytes: bytes.bytes, mime: bytes.mime))
    }

    // MARK: - library dispatch

    private func runLibraryDispatch() async {
        await withTaskGroup(of: Void.self) { group in
            group.addTask { [weak self] in await self?.runLibraryBrowse() }
            group.addTask { [weak self] in await self?.runLibrarySearch() }
            group.addTask { [weak self] in await self?.runLibraryRecommendations() }
            group.addTask { [weak self] in await self?.runLibraryFavoritesList() }
            group.addTask { [weak self] in await self?.runLibraryFavoritesContains() }
            group.addTask { [weak self] in await self?.runLibraryFavoritesToggle() }
            group.addTask { [weak self] in await self?.runLibraryFavoritesSet() }
            group.addTask { [weak self] in await self?.runLibraryFavoritesSetMany() }
        }
    }

    private func runLibraryBrowse() async {
        for await (handle, req) in gateway.library.browseRequests {
            guard let glue = activeGlue else {
                try? await handle.respondErr(LibraryErrorReply(error: Self.noProvider)); continue
            }
            let result: BrowseResult
            do { result = try await glue.browse(req) } catch {
                await Self.failLibrary(error, onProtocol: { try? await handle.respondProtocolErr($0) }, onDomain: { try? await handle.respondErr($0) }); continue
            }
            try? await handle.respond(BrowseReply(result: result))
        }
    }

    private func runLibrarySearch() async {
        for await (handle, req) in gateway.library.searchRequests {
            guard let glue = activeGlue else {
                try? await handle.respondErr(LibraryErrorReply(error: Self.noProvider)); continue
            }
            let result: SearchResult
            do { result = try await glue.search(req) } catch {
                await Self.failLibrary(error, onProtocol: { try? await handle.respondProtocolErr($0) }, onDomain: { try? await handle.respondErr($0) }); continue
            }
            try? await handle.respond(SearchReply(result: result))
        }
    }

    private func runLibraryRecommendations() async {
        for await (handle, req) in gateway.library.recommendationsRequests {
            guard let glue = activeGlue else {
                try? await handle.respondErr(LibraryErrorReply(error: Self.noProvider)); continue
            }
            let result: RecommendationsResult
            do { result = try await glue.recommendations(req) } catch {
                await Self.failLibrary(error, onProtocol: { try? await handle.respondProtocolErr($0) }, onDomain: { try? await handle.respondErr($0) }); continue
            }
            try? await handle.respond(RecommendationsReply(result: result))
        }
    }

    private func runLibraryFavoritesList() async {
        for await (handle, req) in gateway.library.favoritesListRequests {
            guard let glue = activeGlue else {
                try? await handle.respondErr(LibraryErrorReply(error: Self.noProvider)); continue
            }
            let page: FavoritesPage
            do { page = try await glue.favoritesList(req) } catch {
                await Self.failLibrary(error, onProtocol: { try? await handle.respondProtocolErr($0) }, onDomain: { try? await handle.respondErr($0) }); continue
            }
            try? await handle.respond(FavoritesListReply(page: page))
        }
    }

    private func runLibraryFavoritesContains() async {
        for await (handle, req) in gateway.library.favoritesContainsRequests {
            guard let glue = activeGlue else {
                try? await handle.respondErr(LibraryErrorReply(error: Self.noProvider)); continue
            }
            let liked: [Bool]
            do { liked = try await glue.favoritesContains(req) } catch {
                await Self.failLibrary(error, onProtocol: { try? await handle.respondProtocolErr($0) }, onDomain: { try? await handle.respondErr($0) }); continue
            }
            try? await handle.respond(FavoritesContainsReply(liked: liked))
        }
    }

    private func runLibraryFavoritesToggle() async {
        for await (_, msg) in gateway.library.favoritesToggle {
            guard let glue = activeGlue else { continue }
            do { try await glue.favoritesToggle(msg.item) } catch {
                log(.warn, "favoritesToggle failed: \(error.localizedDescription)")
            }
        }
    }

    private func runLibraryFavoritesSet() async {
        for await (_, msg) in gateway.library.favoritesSet {
            guard let glue = activeGlue else { continue }
            do { try await glue.favoritesSet(msg.item, liked: msg.liked) } catch {
                log(.warn, "favoritesSet failed: \(error.localizedDescription)")
            }
        }
    }

    private func runLibraryFavoritesSetMany() async {
        for await (_, msg) in gateway.library.favoritesSetMany {
            guard let glue = activeGlue else { continue }
            do { try await glue.favoritesSetMany(msg.entries) } catch {
                log(.warn, "favoritesSetMany failed: \(error.localizedDescription)")
            }
        }
    }

    private static let noProvider = LibraryError.notSupported(
        LibraryErrorNotSupportedInner(reason: "no active music provider")
    )

    /// Map a glue error onto the right wire response: `notImplemented` -> a
    /// protocol `Unimplemented` (recognized verb, no backend), everything else
    /// -> a `LibraryError` domain reply.
    private static func failLibrary(
        _ error: Error,
        onProtocol: (WireError) async -> Void,
        onDomain: (LibraryErrorReply) async -> Void
    ) async {
        guard let glueError = error as? GlueError else {
            await onDomain(LibraryErrorReply(error: .notSupported(LibraryErrorNotSupportedInner(reason: String(describing: error)))))
            return
        }
        switch glueError {
        case .notImplemented:
            await onProtocol(.unimplemented)
        case .notAuthenticated:
            await onDomain(LibraryErrorReply(error: .unauthorized))
        case .detached:
            await onDomain(LibraryErrorReply(error: .notSupported(LibraryErrorNotSupportedInner(reason: "music provider detached"))))
        case let .underlying(inner):
            await onDomain(LibraryErrorReply(error: .notSupported(LibraryErrorNotSupportedInner(reason: String(describing: inner)))))
        }
    }

    private func runLyricsDispatch() async {
        for await (handle, req) in gateway.lyrics.getRequests {
            await handleLyrics(handle: handle, req: req)
        }
    }

    private func runAncsAuthDispatch() async {
        for await update in gateway.notifications.ancsAuthStateChanged {
            await handleAncsAuthState(update.msg)
        }
    }

    private func handleAncsAuthState(_ next: AncsAuthState) async {
        guard ancsAuthState != next else { return }
        ancsAuthState = next
        log(.info, "ancs auth state -> \(String(describing: next))")
        #if os(iOS)
            if let coordinator = ancsCoordinator {
                await coordinator.setLastAuthState(next)
            }
        #endif
        ancsAuthStateObserver?(next)
    }

    private func handleLyrics(handle: LyricsRequestHandle, req: LyricsRequest) async {
        let identity = BridgethingLyrics.TrackIdentity(
            artist: req.track.artist,
            track: req.track.track,
            album: req.track.album,
            durationMs: req.track.durationMs.map(Int.init),
            isrc: req.track.isrc
        )

        let resolved: BridgethingLyrics.Lyrics?
        do {
            if let glue = activeGlue, let provided = try await glue.lyrics(for: identity) {
                resolved = provided
            } else {
                resolved = await lyricsResolver.lyrics(for: identity)
            }
        } catch {
            log(.warn, "lyrics resolve failed for \(req.track.artist) - \(req.track.track): \(error.localizedDescription)")
            try? await handle.respondErr(LyricsErrorReply(message: String(describing: error)))
            return
        }

        let wire = resolved.map(Self.toWireLyrics)
        try? await handle.respond(LyricsReply(lyrics: wire))
    }

    private func broadcastVolume(level: Float, muted: Bool) async {
        try? await gateway.audio.volumeChanged(VolumeChanged(level: level, muted: muted))
    }

    // MARK: - helpers

    private static func toWireLyrics(_ lyrics: BridgethingLyrics.Lyrics) -> BridgethingSchema.Lyrics {
        BridgethingSchema.Lyrics(
            synced: lyrics.synced?.map { line in
                BridgethingSchema.LyricLine(
                    startMs: UInt32(max(line.startMs, 0)),
                    text: line.text
                )
            },
            plain: lyrics.plain,
            source: lyrics.source
        )
    }
}
