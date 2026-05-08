import BridgethingGateway
import BridgethingGlue
import BridgethingLyrics
import BridgethingSchema
import Foundation

/// Identity the companion advertises in `GatewayCapabilities.gateway`.
/// Caller-supplied at companion init.
public struct HostInfo: Sendable {
    public let appName: String
    public let appVersion: String
    public let osName: String

    public init(appName: String, appVersion: String, osName: String) {
        self.appName = appName
        self.appVersion = appVersion
        self.osName = osName
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
        capabilities: CompanionCapabilityFlags = CompanionCapabilityFlags()
    ) {
        self.host = host
        self.lyricsResolver = lyricsResolver
        capFlags = capabilities
        gateway = BridgethingGateway(adapter: adapter)
        netDispatcher = NetDispatcher()
        ota = OtaService()
        #if canImport(CoreLocation)
            geoController = GeoController()
        #endif
        #if os(iOS)
            volumeMonitor = VolumeMonitor()
        #endif
    }

    public func start() async throws {
        if started { return }
        try await gateway.start()
        started = true

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
    }

    public func setActive(_ glue: (any BridgethingGlue)?) async throws {
        if let activeGlue {
            await activeGlue.detach()
            nowPlayingObserver?(nil)
        }
        activeGlue = glue
        if let glue {
            if let observer = nowPlayingObserver {
                await glue.setNowPlayingObserver(observer)
            }
            try await glue.attach(gateway: gateway)
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

        private func makeOrReuseCoordinator() async -> AncsPairCoordinator {
            if let existing = ancsCoordinator { return existing }
            let coordinator = await MainActor.run { AncsPairCoordinator() }
            ancsCoordinator = coordinator
            return coordinator
        }
    #endif

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
            address: "",
            name: host.appName,
            osName: host.osName,
            appName: host.appName,
            appVersion: host.appVersion,
            adapterVersion: "ea",
            libVersion: "",
            libbridgethingVersion: ""
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
            if case .connected = event {
                await announceCapabilities()
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
            await dispatchPlayer(player, to: glue)
        }
    }

    private nonisolated func dispatchPlayer(
        _ player: BridgeToGatewayPlayerMsg, to glue: any BridgethingGlue
    ) async {
        do {
            switch player {
            case let .play(p): try await glue.play(p)
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
            case .queue: throw GlueError.notImplemented
            }
        } catch {
            // Player verbs are commands; we don't have a response surface
            // to report failures back. Logging only.
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
            try? await handle.respondErr(AssetNotFoundReply(id: id))
            return
        }
        guard let bytes else {
            try? await handle.respondErr(AssetNotFoundReply(id: id))
            return
        }
        try? await handle.respond(AssetGotReply(id: id, bytes: bytes.bytes, mime: bytes.mime))
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
