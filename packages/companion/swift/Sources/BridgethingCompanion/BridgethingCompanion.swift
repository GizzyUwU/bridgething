import BridgethingGateway
import BridgethingGlue
import BridgethingLyrics
import BridgethingSchema
import Foundation
import Logging
#if os(iOS)
    import ExternalAccessory
    import UIKit
#endif

private let osLog = Logger(label: "com.bridgething.companion.core")

public enum CompanionLogLevel: String, Sendable {
    case debug, info, warn, error
}

public enum BridgethingCompanionVersion {
    public static let lib: String = "0.1.0"
    public static let libbridgething: String = "0.1.0"
}

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
        audioTts: Bool = true
    ) {
        self.geo = geo
        self.notifications = notifications
        self.netFetch = netFetch
        self.netWs = netWs
        self.audioTts = audioTts
    }
}

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
    private var deviceLogStreaming = false
    private var localLogStreaming = false
    private var connectedDeviceIds: Set<String> = []
    private var deviceLogTokens: [String: String] = [:]
    private var deviceLogTask: Task<Void, Never>?

    private let netDispatcher: NetDispatcher
    private let tunnelDispatcher: TunnelDispatcher
    private let audioDispatcher: AudioDispatcher
    private var timeChangeObservers: [NSObjectProtocol] = []
    public let ota: OtaService
    public let catalog: CatalogService
    #if canImport(CoreLocation)
        private let geoController: GeoController
    #endif
    #if os(iOS)
        private let audioKeepAlive = BackgroundAudioKeepAlive()
    #endif
    // one ack ledger shared by the asset lane and ota, owned by ota; the collector below feeds it.
    private var transferAcks: TransferAckWindow { ota.transferAcks }

    public init(
        adapter: any Adapter,
        lyricsResolver: any LyricsResolver,
        host: HostInfo,
        capabilities: CompanionCapabilityFlags = CompanionCapabilityFlags(),
        geoProvider: (any GeoLocationProviding)? = nil,
        audioBackend: (any AudioBackend)? = nil
    ) {
        self.host = host
        self.lyricsResolver = lyricsResolver
        capFlags = capabilities
        gateway = BridgethingGateway(adapter: adapter)
        netDispatcher = NetDispatcher()
        tunnelDispatcher = TunnelDispatcher()
        #if canImport(AVFoundation)
            audioDispatcher = AudioDispatcher(backend: audioBackend ?? AvAudioBackend())
        #else
            audioDispatcher = AudioDispatcher(backend: audioBackend ?? NoOpAudioBackend())
        #endif
        ota = OtaService()
        catalog = CatalogService(installer: ota)
        #if canImport(CoreLocation)
            geoController = GeoController(provider: geoProvider)
        #endif
    }

    public func start() async throws {
        if started { return }
        try await gateway.start()
        started = true
        log(.info, "companion started")

        spawnDispatchers()

        #if canImport(Darwin)
            // The device has no battery RTC; re-seed its clock when the phone's
            // timezone or wall clock changes mid-session.
            for name in [Notification.Name.NSSystemTimeZoneDidChange, .NSSystemClockDidChange] {
                let token = NotificationCenter.default.addObserver(
                    forName: name, object: nil, queue: nil
                ) { [weak self] _ in
                    Task { await self?.emitTimeSnapshot() }
                }
                timeChangeObservers.append(token)
            }
        #endif

    }

    public func stop() async {
        for task in tasks {
            task.cancel()
        }
        tasks.removeAll()

        deviceLogTask?.cancel()
        deviceLogTask = nil
        deviceLogTokens.removeAll()
        connectedDeviceIds.removeAll()
        deviceLogStreaming = false
        localLogStreaming = false
        refreshLocalLogSink()

        #if os(iOS)
            await audioKeepAlive.deactivate()
        #endif
        #if canImport(CoreLocation)
            await geoController.stop()
        #endif
        #if canImport(Darwin)
            for token in timeChangeObservers { NotificationCenter.default.removeObserver(token) }
            timeChangeObservers.removeAll()
        #endif
        await netDispatcher.stop()
        await tunnelDispatcher.stop()
        await audioDispatcher.stop()
        await ota.stop()
        await catalog.stop()

        if let glue = activeGlue {
            await glue.detach()
        }
        activeGlue = nil

        await gateway.stop()
        started = false
        log(.info, "companion stopped")
    }

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

    public func setNowPlayingObserver(_ observer: (@Sendable (GlueNowPlaying?) -> Void)?) async {
        nowPlayingObserver = observer
        if let glue = activeGlue {
            await glue.setNowPlayingObserver(observer ?? { _ in })
        }
    }

    public func setAncsAuthStateObserver(_ observer: (@Sendable (AncsAuthState) -> Void)?) {
        ancsAuthStateObserver = observer
    }

    public func setLogObserver(_ observer: (@Sendable (CompanionLogLevel, String) -> Void)?) {
        logObserver = observer
        refreshLocalLogSink()
    }

    private func refreshLocalLogSink() {
        guard localLogStreaming, let observer = logObserver else {
            LocalLogRelay.shared.setSink(nil)
            return
        }
        LocalLogRelay.shared.setSink { level, target, message in
            let companionLevel: CompanionLogLevel = switch level {
            case "ERROR": .error
            case "WARN": .warn
            case "INFO": .info
            default: .debug
            }
            let line = "[\(target)] \(message)"
            DeviceLogRing.shared.push(level: companionLevel.rawValue, message: line)
            observer(companionLevel, line)
        }
    }

    // MARK: - device log streaming

    public func setLocalLogStreaming(_ enabled: Bool) {
        guard enabled != localLogStreaming else { return }
        localLogStreaming = enabled
        refreshLocalLogSink()
    }

    public func setDeviceLogStreaming(_ enabled: Bool) async {
        guard enabled != deviceLogStreaming else { return }
        deviceLogStreaming = enabled
        if enabled {
            startDeviceLogConsumer()
            for id in connectedDeviceIds {
                await subscribeDeviceLogs(id)
            }
        } else {
            deviceLogTask?.cancel()
            deviceLogTask = nil
            let tokens = deviceLogTokens
            deviceLogTokens.removeAll()
            for token in tokens.values {
                try? await gateway.system.logsUnsubscribe(LogsUnsubscribe(token: token))
            }
        }
    }

    private func startDeviceLogConsumer() {
        guard deviceLogTask == nil else { return }
        let stream = gateway.system.logEntry
        deviceLogTask = Task { [weak self] in
            for await (_, entry) in stream {
                await self?.forwardDeviceLog(entry)
            }
        }
    }

    private func subscribeDeviceLogs(_ deviceId: String) async {
        let result = try? await gateway.system.logsSubscribe(
            deviceId: deviceId,
            LogsSubscribe(source: .daemon, levels: [], filter: nil)
        )
        if case .ok(let reply) = result {
            deviceLogTokens[deviceId] = reply.token
        }
    }

    private func forwardDeviceLog(_ entry: LogEntry) {
        let level: CompanionLogLevel = switch entry.level {
        case .trace, .debug: .debug
        case .info: .info
        case .warn: .warn
        case .error: .error
        }
        let message = "[\(entry.target)] \(entry.message)"
        DeviceLogRing.shared.push(level: level.rawValue, message: message)
        logObserver?(level, message)
    }

    nonisolated func emitLog(_ level: CompanionLogLevel, _ message: String, observer: (@Sendable (CompanionLogLevel, String) -> Void)?) {
        switch level {
        case .debug: osLog.debug("\(message)")
        case .info: osLog.info("\(message)")
        case .warn: osLog.warning("\(message)")
        case .error: osLog.error("\(message)")
        }
        DeviceLogRing.shared.push(level: level.rawValue, message: message)
        observer?(level, message)
    }

    public func currentAncsAuthState() -> AncsAuthState {
        ancsAuthState
    }

    public func enableAncsNotifications() async -> AncsSetupResult {
        #if os(iOS)
            log(.info, "enableAncsNotifications: acquiring coordinator")
            let coordinator = await makeOrReuseCoordinator()
            await coordinator.setLastAuthState(ancsAuthState)
            let result = await coordinator.pair()
            log(.info, "enableAncsNotifications: result \(String(describing: result.kind))")
            return result
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

        private func reestablishAncsLink() async {
            let coordinator = await makeOrReuseCoordinator()
            await coordinator.setLastAuthState(ancsAuthState)
            await coordinator.reconnectIfPaired()
        }
    #endif

    public func presentPairPicker() async -> AccessoryPickResult? {
        #if os(iOS)
            // the gateway rides iAP2/EA over BR/EDR, which needs a classic bond + MFi auth that
            // AccessorySetupKit (LE/wifi only) can't do; the EA picker is the path for an MFi accessory.
            return await Self.presentBluetoothAccessoryPicker()
        #else
            return nil
        #endif
    }

    #if os(iOS)
        private static func presentBluetoothAccessoryPicker() async -> AccessoryPickResult? {
            await withCheckedContinuation { (cont: CheckedContinuation<AccessoryPickResult?, Never>) in
                Task { @MainActor in
                    osLog.info("presenting EA bluetooth accessory picker")
                    EAAccessoryManager.shared().showBluetoothAccessoryPicker(withNameFilter: nil) { error in
                        guard let error else {
                            osLog.info("EA picker completed")
                            cont.resume(returning: AccessoryPickResult(id: "", name: "Bridgething"))
                            return
                        }
                        let ns = error as NSError
                        if ns.domain == EABluetoothAccessoryPickerErrorDomain,
                            let code = EABluetoothAccessoryPickerError.Code(rawValue: ns.code)
                        {
                            switch code {
                            case .alreadyConnected:
                                osLog.info("EA picker: accessory already connected")
                                cont.resume(returning: AccessoryPickResult(id: "", name: "Bridgething"))
                                return
                            case .resultNotFound:
                                osLog.warning("EA picker: no accessory found")
                            case .resultCancelled:
                                // ios returns cancelled for both a user dismiss and a select-then-bond/auth failure
                                osLog.warning("EA picker dismissed without pairing")
                            case .resultFailed:
                                osLog.warning("EA picker: pairing failed")
                            @unknown default:
                                osLog.warning("EA picker error: \(error.localizedDescription)")
                            }
                        } else {
                            osLog.warning("EA picker error: \(error.localizedDescription)")
                        }
                        cont.resume(returning: nil)
                    }
                }
            }
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
        tasks.append(Task { [weak self] in await self?.runKeepaliveResponder() })
        tasks.append(Task { [weak self] in
            guard let self else { return }
            for await (_, ack) in gateway.transfer.ack {
                await transferAcks.note(transferId: ack.transferId, received: ack.received)
            }
        })
        #if os(iOS)
            tasks.append(Task { [weak self] in
                for await _ in NotificationCenter.default.notifications(
                    named: UIApplication.didBecomeActiveNotification
                ) {
                    guard let self else { return }
                    await self.reestablishAncsLink()
                }
            })
        #endif
        tasks.append(Task { [weak self] in await self?.runPlayerDispatch() })
        tasks.append(Task { [weak self] in await self?.runAssetDispatch() })
        tasks.append(Task { [weak self] in await self?.runLibraryDispatch() })
        tasks.append(Task { [weak self] in await self?.runLyricsDispatch() })
        tasks.append(Task { [weak self] in await self?.runAncsAuthDispatch() })
        tasks.append(Task { [weak self] in await self?.runWebappProfileDispatch() })
        tasks.append(Task { [weak self] in
            guard let self else { return }
            await netDispatcher.start(gateway: gateway)
        })
        tasks.append(Task { [weak self] in
            guard let self else { return }
            await tunnelDispatcher.start(gateway: gateway)
        })
        tasks.append(Task { [weak self] in
            guard let self else { return }
            await audioDispatcher.setGlueProvider { [weak self] in await self?.current() }
            await audioDispatcher.start(gateway: gateway)
        })
        tasks.append(Task { [weak self] in
            guard let self else { return }
            await ota.start(gateway: gateway)
        })
        tasks.append(Task { [weak self] in
            guard let self else { return }
            await catalog.start(gateway: gateway)
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
                let wasEmpty = connectedDeviceIds.isEmpty
                connectedDeviceIds.insert(device.id)
                if deviceLogStreaming { await subscribeDeviceLogs(device.id) }
                await announceCapabilities()
                await emitTimeSnapshot()
                await activeGlue?.handlePeerConnected()
                #if os(iOS)
                    await reestablishAncsLink()
                    if wasEmpty { await audioKeepAlive.activate() }
                #endif
            case let .disconnected(id):
                await handlePeerGone(id, reason: "disconnected")
            case let .linkFailed(device, reason):
                log(.warn, "peer link failed: \(device.name) [\(device.id)]: \(reason)")
                await handlePeerGone(device.id, reason: "linkFailed")
            case let .decodeError(id, description):
                log(.warn, "[\(id)] decode error: \(description)")
            case .message:
                continue
            }
        }
    }

    private func handlePeerGone(_ id: String, reason: String) async {
        guard connectedDeviceIds.remove(id) != nil else { return }
        log(.info, "peer gone (\(reason)): \(id)")
        deviceLogTokens.removeValue(forKey: id)
        #if os(iOS)
            if connectedDeviceIds.isEmpty { await audioKeepAlive.deactivate() }
        #endif
    }

    private func runKeepaliveResponder() async {
        for await (handle, req) in gateway.system.keepaliveRequests {
            try? await handle.respond(KeepaliveAck(seq: req.seq))
        }
    }

    private func runWebappProfileDispatch() async {
        for await (_, changed) in gateway.webapp.activeChanged {
            let hero = changed.art.map { Int($0.heroPx) } ?? 248
            let thumb = changed.art.map { Int($0.thumbPx) } ?? 96
            await activeGlue?.setArtProfile(heroPx: hero, thumbPx: thumb)
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
            Task { [weak self] in await self?.handleAsset(handle: handle, id: req.id, requestId: req.requestId) }
        }
    }

    private static let assetFragmentBytes = 4 * 1024
    private static let inlineBodyMaxBytes = 8 * 1024
    private static let transferWindowBytes = 8 * 1024
    private static let transferAckTimeoutSeconds: Double = 15

    private func handleAsset(handle: AssetRequestHandle, id: String, requestId: UUID) async {
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
        do {
            try await streamAsset(handle: handle, id: id, requestId: requestId, payload: bytes)
        } catch {
            log(.warn, "asset \(id) respond failed: \(error.localizedDescription)")
        }
    }

    private func streamAsset(handle: AssetRequestHandle, id: String, requestId: UUID, payload: AssetBytes) async throws {
        let data = payload.bytes
        if data.count <= Self.inlineBodyMaxBytes {
            try await handle.respond(AssetGotReply(id: id, mime: payload.mime, body: .inline(data)))
            return
        }
        try await handle.respond(
            AssetGotReply(
                id: id,
                mime: payload.mime,
                body: .stream(TransferRef(id: requestId, totalSize: UInt32(data.count), sha256: nil))
            ),
            priority: .normal
        )
        try await Self.sendFragments(
            surface: gateway.device(handle.deviceId).transfer,
            transferId: requestId,
            data: data,
            fragmentBytes: Self.assetFragmentBytes,
            priority: .background,
            acks: transferAcks
        )
    }

    static func sendFragments(
        surface: TransferSurfaceForDevice,
        transferId: UUID,
        data: Data,
        fragmentBytes: Int,
        priority: Priority,
        acks: TransferAckWindow? = nil
    ) async throws {
        var offset = 0
        while offset < data.count {
            if let acks {
                while true {
                    let acked = await acks.receivedBytes(transferId)
                    if offset < Int(acked) + Self.transferWindowBytes { break }
                    guard await acks.waitForProgress(
                        transferId, beyond: acked, timeoutSeconds: Self.transferAckTimeoutSeconds
                    ) else {
                        await acks.finish(transferId)
                        try? await surface.abandon(TransferAbandon(transferId: transferId, reason: "ack timeout"))
                        throw TransferStalled()
                    }
                }
            }
            let end = min(offset + fragmentBytes, data.count)
            try await surface.fragment(
                TransferFragment(transferId: transferId, offset: UInt32(offset), bytes: data.subdata(in: offset ..< end)),
                priority: priority
            )
            offset = end
        }
        if let acks { await acks.finish(transferId) }
    }

    // MARK: - library dispatch

    private func runLibraryDispatch() async {
        await withTaskGroup(of: Void.self) { group in
            group.addTask { [weak self] in await self?.runLibraryBrowse() }
            group.addTask { [weak self] in await self?.runLibraryResolveContext() }
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
            Task { [weak self] in
                guard let glue = await self?.activeGlue else {
                    try? await handle.respondErr(LibraryErrorReply(error: Self.noProvider)); return
                }
                do {
                    let isRoot = req.nodeId == nil || req.nodeId == "" || req.nodeId == "root"
                    try await handle.respond(
                        BrowseReply(result: glue.browse(req)),
                        priority: isRoot ? .bulk : nil,
                        compression: isRoot ? .gzip : nil
                    )
                } catch {
                    await Self.failLibrary(error, onProtocol: { try? await handle.respondProtocolErr($0) }, onDomain: { try? await handle.respondErr($0) })
                }
            }
        }
    }

    private func runLibraryResolveContext() async {
        for await (handle, req) in gateway.library.resolveContextRequests {
            Task { [weak self] in
                guard let glue = await self?.activeGlue else {
                    try? await handle.respondErr(LibraryErrorReply(error: Self.noProvider)); return
                }
                do {
                    try await handle.respond(glue.resolveContext(req.uri))
                } catch {
                    await Self.failLibrary(error, onProtocol: { try? await handle.respondProtocolErr($0) }, onDomain: { try? await handle.respondErr($0) })
                }
            }
        }
    }

    private func runLibrarySearch() async {
        for await (handle, req) in gateway.library.searchRequests {
            Task { [weak self] in
                guard let glue = await self?.activeGlue else {
                    try? await handle.respondErr(LibraryErrorReply(error: Self.noProvider)); return
                }
                do {
                    try await handle.respond(SearchReply(result: glue.search(req)))
                } catch {
                    await Self.failLibrary(error, onProtocol: { try? await handle.respondProtocolErr($0) }, onDomain: { try? await handle.respondErr($0) })
                }
            }
        }
    }

    private func runLibraryRecommendations() async {
        for await (handle, req) in gateway.library.recommendationsRequests {
            Task { [weak self] in
                guard let glue = await self?.activeGlue else {
                    try? await handle.respondErr(LibraryErrorReply(error: Self.noProvider)); return
                }
                do {
                    try await handle.respond(RecommendationsReply(result: glue.recommendations(req)))
                } catch {
                    await Self.failLibrary(error, onProtocol: { try? await handle.respondProtocolErr($0) }, onDomain: { try? await handle.respondErr($0) })
                }
            }
        }
    }

    private func runLibraryFavoritesList() async {
        for await (handle, req) in gateway.library.favoritesListRequests {
            Task { [weak self] in
                guard let glue = await self?.activeGlue else {
                    try? await handle.respondErr(LibraryErrorReply(error: Self.noProvider)); return
                }
                do {
                    try await handle.respond(FavoritesListReply(page: glue.favoritesList(req)))
                } catch {
                    await Self.failLibrary(error, onProtocol: { try? await handle.respondProtocolErr($0) }, onDomain: { try? await handle.respondErr($0) })
                }
            }
        }
    }

    private func runLibraryFavoritesContains() async {
        for await (handle, req) in gateway.library.favoritesContainsRequests {
            Task { [weak self] in
                guard let glue = await self?.activeGlue else {
                    try? await handle.respondErr(LibraryErrorReply(error: Self.noProvider)); return
                }
                do {
                    try await handle.respond(FavoritesContainsReply(liked: glue.favoritesContains(req)))
                } catch {
                    await Self.failLibrary(error, onProtocol: { try? await handle.respondProtocolErr($0) }, onDomain: { try? await handle.respondErr($0) })
                }
            }
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
            Task { [weak self] in await self?.handleLyrics(handle: handle, req: req) }
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

    private func emitTimeSnapshot() async {
        try? await gateway.time.snapshot(Self.currentTimeInfo())
    }

    private static func currentTimeInfo() -> TimeInfo {
        let now = Date()
        let tz = TimeZone.current
        return TimeInfo(
            tzIana: tz.identifier,
            locale: Locale.current.identifier,
            wallClockUnixS: UInt32(clamping: Int(now.timeIntervalSince1970)),
            utcOffsetMinutes: Int16(clamping: tz.secondsFromGMT(for: now) / 60),
            dstOffsetMinutes: Int8(clamping: Int(tz.daylightSavingTimeOffset(for: now)) / 60)
        )
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
