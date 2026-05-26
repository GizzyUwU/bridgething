import BridgethingCompanion
import BridgethingGateway
import BridgethingGlue
import BridgethingLyrics
import BridgethingSchema
import BridgethingSession
import CryptoKit
import Foundation
import NitroModules
import UIKit

/// Real `BridgethingSessionBackend` for the bridgething host app.
/// Owns one `BridgethingCompanion` and translates `GlueAuthState` updates
/// into the wire `BridgethingAuthState`. This backend persists nothing; JS
/// owns preferences in mmkv and reapplies them on bootstrap.
public final class HybridBridgethingSessionImpl: BridgethingSessionBackend, @unchecked Sendable {
    public typealias GlueFactory = @Sendable () -> any BridgethingGlue
    public typealias SignOutFn = @Sendable () -> Void

    public struct ProviderRegistration: Sendable {
        public let id: String
        public let displayName: String
        public let available: Bool
        public let factory: GlueFactory
        public let signOut: SignOutFn

        public init(
            id: String,
            displayName: String,
            available: Bool,
            factory: @escaping GlueFactory,
            signOut: @escaping SignOutFn
        ) {
            self.id = id
            self.displayName = displayName
            self.available = available
            self.factory = factory
            self.signOut = signOut
        }
    }

    public static var registry: [ProviderRegistration] = []
    public static var hostInfo: HostInfo = .init(appName: "bridgething", appVersion: "0.0.0", osName: "iOS")
    public static var lyricsResolver: any LyricsResolver = LrclibResolver()
    public static var eaProtocolString: String = "com.bridgething.gateway"

    private let stateLock = NSLock()
    private var companion: BridgethingCompanion?
    private var eventsTask: Task<Void, Never>?
    private var authTask: Task<Void, Never>?
    private var otaEventsTask: Task<Void, Never>?
    private var peers: [String: BridgethingSessionPeer] = [:]
    private var lastNowPlaying: BridgethingNowPlaying?
    private var activeRegistration: ProviderRegistration?

    private var onProviderChanged: (@Sendable (BridgethingProviderInfo?) -> Void)?
    private var onAuthStateChanged: (@Sendable (BridgethingAuthState) -> Void)?
    private var onPeerConnected: (@Sendable (BridgethingSessionPeer) -> Void)?
    private var onPeerDisconnected: (@Sendable (String) -> Void)?
    private var onNowPlayingChanged: (@Sendable (BridgethingNowPlaying?) -> Void)?
    private var onAncsAuthStatusChanged: (@Sendable (BridgethingAncsAuthStatus) -> Void)?
    private var onLog: (@Sendable (String, String) -> Void)?
    private var onWebappsChanged: (@Sendable (String) -> Void)?
    private var onDeviceMetaChanged: (@Sendable (String, BridgethingDeviceMeta) -> Void)?
    private var onOtaEvent: (@Sendable (BridgethingOtaEvent) -> Void)?
    private var logStreamingDesired: Bool = false

    public init() {}

    // MARK: - Lifecycle

    public func start() async throws {
        if stateLock.withLock({ self.companion != nil }) { return }
        let adapter = EAAccessoryAdapter(protocolString: Self.eaProtocolString)
        let host = Self.makeHostInfo()
        // capability flags start all-off; JS applies them via setCapabilityFlags on bootstrap
        let companion = BridgethingCompanion(
            adapter: adapter,
            lyricsResolver: Self.lyricsResolver,
            host: host,
            capabilities: CompanionCapabilityFlags()
        )
        stateLock.lock(); self.companion = companion; stateLock.unlock()

        await companion.setNowPlayingObserver { [weak self] np in
            self?.handleNowPlaying(np)
        }
        await companion.setAncsAuthStateObserver { [weak self] state in
            self?.emitAncsAuthStatus(toRNAncsAuthStatus(state))
        }
        if stateLock.withLock({ logStreamingDesired }) {
            await companion.setLogObserver { [weak self] level, message in
                self?.emitLog(level.rawValue, message)
            }
        }

        try await companion.start()

        let events = companion.gateway.events
        let task = Task { [weak self] in
            for await event in events {
                self?.handleGatewayEvent(event)
            }
        }
        let ota = await companion.ota
        let otaStream = ota.events
        let otaTask = Task { [weak self] in
            for await event in otaStream {
                self?.emitOtaEvent(toRNOtaEvent(event))
            }
        }
        stateLock.lock()
        eventsTask = task
        otaEventsTask = otaTask
        stateLock.unlock()
    }

    public func stop() async {
        stateLock.lock()
        let auth = authTask
        let events = eventsTask
        let ota = otaEventsTask
        let companion = self.companion
        self.companion = nil
        eventsTask = nil
        otaEventsTask = nil
        authTask = nil
        stateLock.unlock()

        auth?.cancel()
        events?.cancel()
        ota?.cancel()

        await companion?.stop()

        stateLock.lock()
        peers.removeAll()
        lastNowPlaying = nil
        stateLock.unlock()
        emitNowPlaying(nil)
    }

    // MARK: - Provider selection

    public func availableProviders() async -> [BridgethingProviderInfo] {
        Self.registry.map {
            BridgethingProviderInfo(id: $0.id, displayName: $0.displayName, available: $0.available)
        }
    }

    public func setActiveProvider(id: String?) async throws {
        stateLock.lock(); let prevTask = authTask; stateLock.unlock()
        prevTask?.cancel()

        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            let task = Task { [weak self] in
                guard let self else {
                    continuation.resume(throwing: SessionError.deallocated)
                    return
                }
                do {
                    try await runSetActive(id: id)
                    continuation.resume()
                } catch is CancellationError {
                    emitAuth(.idleState())
                    continuation.resume(throwing: SessionError.cancelled)
                } catch {
                    emitAuth(.failed(message: String(describing: error)))
                    continuation.resume(throwing: error)
                }
            }
            stateLock.lock(); authTask = task; stateLock.unlock()
        }
    }

    public func cancelAuth() async {
        stateLock.lock()
        let task = authTask
        activeRegistration = nil
        stateLock.unlock()
        task?.cancel()
        let companion = stateLock.withLock { self.companion }
        try? await companion?.setActive(nil)
        emitProvider(nil)
        emitAuth(.idleState())
    }

    public func signOut() async {
        stateLock.lock()
        let task = authTask
        activeRegistration = nil
        stateLock.unlock()
        task?.cancel()

        let companion = stateLock.withLock { self.companion }
        let glue = await companion?.current()

        if let glue {
            let providerId = type(of: glue).name
            if let registration = Self.registry.first(where: { $0.id == providerId }) {
                registration.signOut()
            }
        }

        try? await companion?.setActive(nil)
        emitProvider(nil)
        emitAuth(.idleState())
    }

    public func currentProvider() async -> BridgethingProviderInfo? {
        let companion = stateLock.withLock { self.companion }
        let glue = await companion?.current()
        return providerInfo(for: glue)
    }

    public func connectedPeers() async -> [BridgethingSessionPeer] {
        stateLock.withLock { Array(peers.values) }
    }

    public func currentNowPlaying() async -> BridgethingNowPlaying? {
        stateLock.withLock { lastNowPlaying }
    }

    // MARK: - ANCS

    public func enableAncsNotifications() async -> BridgethingAncsSetupResult {
        let companion = stateLock.withLock { self.companion }
        guard let companion else {
            return BridgethingAncsSetupResult(
                kind: .failed,
                authStatus: .unknown,
                message: "session not started"
            )
        }
        let result = await companion.enableAncsNotifications()
        return toRNAncsSetupResult(result)
    }

    public func ancsAuthStatus() async -> BridgethingAncsAuthStatus {
        let companion = stateLock.withLock { self.companion }
        guard let companion else { return .unknown }
        return toRNAncsAuthStatus(await companion.currentAncsAuthState())
    }

    // MARK: - Webapps (per-device)

    public func listWebapps(deviceId: String) async throws -> [BridgethingWebappInfo] {
        let companion = try requirePeerConnected(deviceId)
        let result = try await companion.gateway.webapp.list(deviceId: deviceId)
        let value = try unwrapVoidErr(result, label: "listWebapps")
        return value.webapps
            .filter { $0.role != .launcher }
            .map(Self.toRNWebappInfo)
    }

    public func currentWebapp(deviceId: String) async throws -> BridgethingActiveWebapp? {
        let companion = try requirePeerConnected(deviceId)
        let result = try await companion.gateway.webapp.getActive(deviceId: deviceId)
        let value = try unwrapVoidErr(result, label: "currentWebapp")
        guard let id = value.id else { return nil }
        return BridgethingActiveWebapp(id: id.uuidString.lowercased(), name: value.name)
    }

    public func installWebappFromBase64(deviceId: String, archiveBase64: String) async throws -> BridgethingWebappInfo {
        guard let data = Data(base64Encoded: archiveBase64, options: .ignoreUnknownCharacters) else {
            throw SessionError.invalidArchive
        }
        guard data.count <= Int(UInt32.max) else {
            throw SessionError.invalidArchive
        }
        let companion = try requirePeerConnected(deviceId)
        let installId = Self.sha256Hex(data)
        let begin = WebappInstallBegin(
            installId: installId,
            expectedSha256: installId,
            expectedSize: UInt32(data.count)
        )
        let beginResult = try await companion.gateway.webapp.installBegin(deviceId: deviceId, begin)
        let ack = try unwrapWebappErr(beginResult, label: "installBegin")

        // subscribe before the last chunk lands to avoid racing the daemon's broadcast
        let installedTask = Task<WebappInfo, Error> {
            for await pair in companion.gateway.webapp.webappInstalled where pair.deviceId == deviceId {
                return pair.msg
            }
            throw SessionError.installInterrupted
        }
        let failedTask = Task<WebappError, Error> {
            for await pair in companion.gateway.webapp.webappInstallFailed
                where pair.deviceId == deviceId && pair.msg.installId == installId {
                return pair.msg.error
            }
            throw SessionError.installInterrupted
        }

        do {
            try await streamInstallChunks(
                gateway: companion.gateway,
                deviceId: deviceId,
                installId: installId,
                data: data,
                startOffset: ack.resumeFromOffset
            )
        } catch {
            installedTask.cancel()
            failedTask.cancel()
            throw error
        }

        let info: WebappInfo
        do {
            info = try await withThrowingTaskGroup(of: InstallOutcome.self) { group in
                group.addTask { .installed(try await installedTask.value) }
                group.addTask { .failed(try await failedTask.value) }
                group.addTask {
                    try await Task.sleep(nanoseconds: 60_000_000_000)
                    throw SessionError.installTimedOut
                }
                defer { group.cancelAll() }
                guard let first = try await group.next() else {
                    throw SessionError.installInterrupted
                }
                switch first {
                case let .installed(value):
                    return value
                case let .failed(err):
                    throw SessionError.webappError(err)
                }
            }
        } catch {
            installedTask.cancel()
            failedTask.cancel()
            throw error
        }

        emitWebappsChanged(deviceId)
        return Self.toRNWebappInfo(info)
    }

    private func streamInstallChunks(
        gateway: BridgethingGateway,
        deviceId: String,
        installId: String,
        data: Data,
        startOffset: UInt32
    ) async throws {
        let chunkSize = 64 * 1024
        let total = data.count
        var offset = Int(startOffset)
        while offset < total {
            let end = min(offset + chunkSize, total)
            let slice = data.subdata(in: offset..<end)
            let last = end == total
            let chunk = WebappInstallChunk(
                installId: installId,
                offset: UInt32(offset),
                bytes: slice,
                last: last
            )
            try await gateway.device(deviceId).webapp.installChunk(chunk, priority: .bulk)
            offset = end
        }
    }

    private static func sha256Hex(_ data: Data) -> String {
        var hasher = SHA256()
        hasher.update(data: data)
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }

    public func uninstallWebapp(deviceId: String, id: String) async throws {
        let uuid = try parseUuid(id)
        let companion = try requirePeerConnected(deviceId)
        let req = WebappUninstall(id: uuid)
        let result = try await companion.gateway.webapp.uninstall(deviceId: deviceId, req)
        _ = try unwrapWebappErr(result, label: "uninstallWebapp")
        emitWebappsChanged(deviceId)
    }

    public func switchWebapp(deviceId: String, id: String) async throws {
        let uuid = try parseUuid(id)
        let companion = try requirePeerConnected(deviceId)
        let req = WebappSwitchTo(id: uuid)
        let result = try await companion.gateway.webapp.switchTo(deviceId: deviceId, req)
        _ = try unwrapWebappErr(result, label: "switchWebapp")
        emitWebappsChanged(deviceId)
    }

    public func webappIcon(deviceId: String, id: String) async throws -> BridgethingWebappIcon? {
        let uuid = try parseUuid(id)
        let companion = try requirePeerConnected(deviceId)
        let req = WebappIcon(id: uuid)
        let result = try await companion.gateway.webapp.icon(deviceId: deviceId, req)
        switch result {
        case let .ok(reply):
            let url = try Self.writeIconToCache(deviceId: deviceId, id: id, mime: reply.mime, bytes: reply.bytes)
            return BridgethingWebappIcon(fileUri: url.absoluteString, mime: reply.mime)
        case let .domain(err):
            if case .iconNotAvailable = err { return nil }
            throw SessionError.webappError(err)
        case let .protocolError(err):
            throw SessionError.protocolError(err)
        }
    }

    public func listWebappConfig(deviceId: String, id: String) async throws -> [BridgethingConfigEntry] {
        let uuid = try parseUuid(id)
        let companion = try requirePeerConnected(deviceId)
        let req = WebappConfigList(id: uuid)
        let result = try await companion.gateway.webapp.configList(deviceId: deviceId, req)
        let reply = try unwrapWebappErr(result, label: "listWebappConfig")
        return reply.entries.map { BridgethingConfigEntry(key: $0.key, value: $0.value) }
    }

    public func setWebappConfigField(deviceId: String, id: String, key: String, value: String) async throws {
        let uuid = try parseUuid(id)
        let companion = try requirePeerConnected(deviceId)
        let req = WebappConfigSet(id: uuid, key: key, value: value)
        let result = try await companion.gateway.webapp.configSet(deviceId: deviceId, req)
        _ = try unwrapWebappErr(result, label: "setWebappConfigField")
    }

    public func deleteWebappConfigField(deviceId: String, id: String, key: String) async throws {
        let uuid = try parseUuid(id)
        let companion = try requirePeerConnected(deviceId)
        let req = WebappConfigDelete(id: uuid, key: key)
        let result = try await companion.gateway.webapp.configDelete(deviceId: deviceId, req)
        _ = try unwrapWebappErr(result, label: "deleteWebappConfigField")
    }

    // MARK: - Capability flags

    public func setCapabilityFlags(flags: BridgethingCapabilityFlags) async {
        let companionFlags = CompanionCapabilityFlags(
            geo: flags.geo,
            notifications: flags.notifications,
            netFetch: flags.netFetch,
            netWs: flags.netWs,
            audioTts: flags.audioTts
        )
        let companion = stateLock.withLock { self.companion }
        await companion?.setCapabilityFlags(companionFlags)
    }

    // MARK: - OTA

    public func setOtaPollConfig(config: BridgethingOtaPollConfig?) async {
        let companion = stateLock.withLock { self.companion }
        let ota = await companion?.ota
        if let config {
            let mapped = OtaPollConfig(
                rootURL: config.rootUrl.flatMap(URL.init(string:)) ?? URL(string: "https://ota.bridgething.com")!,
                channel: config.channel,
                intervalSeconds: max(60, config.intervalSeconds),
                cacheDirectory: nil,
                autoPush: config.autoPush
            )
            await ota?.setPollConfig(mapped)
        } else {
            await ota?.setPollConfig(nil)
        }
    }

    public func pollOtaNow() async {
        let companion = stateLock.withLock { self.companion }
        let ota = await companion?.ota
        await ota?.pollNow()
    }

    public func deviceMeta(deviceId: String) async -> BridgethingDeviceMeta? {
        let companion = stateLock.withLock { self.companion }
        guard let companion else { return nil }
        let ota = await companion.ota
        guard let meta = await ota.meta(deviceId: deviceId) else { return nil }
        return Self.toRNDeviceMeta(meta)
    }

    // MARK: - Host identity

    public func hostInfo() async -> BridgethingHostInfo {
        let host = Self.makeHostInfo()
        return BridgethingHostInfo(
            appName: host.appName,
            appVersion: host.appVersion,
            osName: host.osName,
            osVersion: host.osVersion,
            hostIdentifier: host.address,
            libVersion: BridgethingCompanionVersion.lib,
            libbridgethingVersion: BridgethingCompanionVersion.libbridgething,
            adapterVersion: host.adapterVersion
        )
    }

    private static func makeHostInfo() -> HostInfo {
        let base = Self.hostInfo
        let identifier = UIDevice.current.identifierForVendor?.uuidString ?? ""
        return HostInfo(
            appName: base.appName,
            appVersion: base.appVersion,
            osName: base.osName,
            osVersion: UIDevice.current.systemVersion,
            address: identifier,
            adapterVersion: "eaccessory"
        )
    }

    // MARK: - Callback setters

    public func setOnProviderChanged(_ callback: @escaping @Sendable (BridgethingProviderInfo?) -> Void) {
        stateLock.withLock { onProviderChanged = callback }
    }

    public func setOnAuthStateChanged(_ callback: @escaping @Sendable (BridgethingAuthState) -> Void) {
        stateLock.withLock { onAuthStateChanged = callback }
    }

    public func setOnPeerConnected(_ callback: @escaping @Sendable (BridgethingSessionPeer) -> Void) {
        stateLock.withLock { onPeerConnected = callback }
    }

    public func setOnPeerDisconnected(_ callback: @escaping @Sendable (String) -> Void) {
        stateLock.withLock { onPeerDisconnected = callback }
    }

    public func setOnNowPlayingChanged(_ callback: @escaping @Sendable (BridgethingNowPlaying?) -> Void) {
        stateLock.withLock { onNowPlayingChanged = callback }
    }

    public func setOnAncsAuthStatusChanged(_ callback: @escaping @Sendable (BridgethingAncsAuthStatus) -> Void) {
        stateLock.withLock { onAncsAuthStatusChanged = callback }
    }

    public func setOnLog(_ callback: @escaping @Sendable (String, String) -> Void) {
        stateLock.withLock { onLog = callback }
    }

    public func setLogStreamingEnabled(_ enabled: Bool) {
        let companion: BridgethingCompanion? = stateLock.withLock {
            logStreamingDesired = enabled
            return self.companion
        }
        guard let companion else { return }
        if enabled {
            Task { [weak self] in
                await companion.setLogObserver { [weak self] level, message in
                    self?.emitLog(level.rawValue, message)
                }
            }
        } else {
            Task {
                await companion.setLogObserver(nil)
            }
        }
    }

    public func setOnWebappsChanged(_ callback: @escaping @Sendable (String) -> Void) {
        stateLock.withLock { onWebappsChanged = callback }
    }

    public func setOnDeviceMetaChanged(_ callback: @escaping @Sendable (String, BridgethingDeviceMeta) -> Void) {
        stateLock.withLock { onDeviceMetaChanged = callback }
    }

    public func setOnOtaEvent(_ callback: @escaping @Sendable (BridgethingOtaEvent) -> Void) {
        stateLock.withLock { onOtaEvent = callback }
    }

    // MARK: - Cross-platform AccessorySetupKit picker

    public func presentPairPicker() async throws -> BridgethingBtDevice? {
        let companion = try requireCompanion()
        guard let result = await companion.presentPairPicker() else { return nil }
        return BridgethingBtDevice(
            address: result.id,
            name: result.name,
            bondState: .bonded,
            isCarThing: true
        )
    }

    // MARK: - Android-only surfaces (iOS stubs)

    public func isNotificationAccessGranted() async -> Bool { false }
    public func requestNotificationAccess() async throws { throw SessionError.unsupportedOnPlatform }
    public func revokeRuntimePermissions(permissions: [String]) async -> Bool { false }
    public func killApp() async {
        // no-op on iOS; Apple rejects explicit process termination
    }

    // MARK: - Internal

    private func runSetActive(id: String?) async throws {
        let companion = stateLock.withLock { self.companion }
        guard let companion else { throw SessionError.notStarted }

        if let id {
            guard let registration = Self.registry.first(where: { $0.id == id }) else {
                throw SessionError.unknownProvider(id)
            }

            let glue = registration.factory()
            stateLock.withLock { activeRegistration = registration }
            // subscribe before setActive; the glue may emit authenticated synchronously during attach
            await glue.setAuthObserver { [weak self] state in
                self?.handleGlueAuthState(state)
            }
            try await companion.setActive(glue)
            try Task.checkCancellation()
        } else {
            stateLock.withLock { activeRegistration = nil }
            try await companion.setActive(nil)
            emitProvider(nil)
            emitAuth(.idleState())
        }
    }

    private func handleGlueAuthState(_ state: GlueAuthState) {
        switch state {
        case let .pending(prompt):
            emitAuth(.pendingState(
                userCode: prompt?.userCode,
                verificationUrl: prompt?.verificationURL.absoluteString,
                verificationUrlComplete: prompt?.verificationURLComplete.absoluteString
            ))
        case .authenticated:
            if let registration = stateLock.withLock({ activeRegistration }) {
                emitProvider(BridgethingProviderInfo(
                    id: registration.id,
                    displayName: registration.displayName,
                    available: registration.available
                ))
            }
            emitAuth(.authenticated())
        case let .failed(message):
            emitAuth(.failed(message: message))
        }
    }

    private func handleGatewayEvent(_ event: GatewayEvent) {
        switch event {
        case let .connected(device):
            let peer = BridgethingSessionPeer(id: device.id, name: device.name)
            stateLock.withLock { peers[device.id] = peer }
            emitPeerConnected(peer)
        case let .disconnected(id):
            stateLock.withLock { _ = peers.removeValue(forKey: id) }
            emitPeerDisconnected(id)
        case let .message(deviceId, msg):
            if case let .version(meta) = msg.data {
                emitDeviceMetaChanged(deviceId, Self.toRNDeviceMeta(meta))
            }
        case let .decodeError(id, description):
            emitLog("warn", "[\(id)] decode error: \(description)")
        }
    }

    private func handleNowPlaying(_ glue: GlueNowPlaying?) {
        let rn: BridgethingNowPlaying? = glue.flatMap(Self.toRNNowPlaying)
        stateLock.withLock { lastNowPlaying = rn }
        emitNowPlaying(rn)
    }

    private func providerInfo(for glue: (any BridgethingGlue)?) -> BridgethingProviderInfo? {
        guard let glue else { return nil }
        let registration = Self.registry.first { $0.id == type(of: glue).name }
        return BridgethingProviderInfo(
            id: type(of: glue).name,
            displayName: registration?.displayName ?? type(of: glue).displayName,
            available: registration?.available ?? true
        )
    }

    private func requirePeerConnected(_ deviceId: String) throws -> BridgethingCompanion {
        let (companion, connected) = stateLock.withLock { (self.companion, peers[deviceId] != nil) }
        guard let companion else { throw SessionError.notStarted }
        guard connected else { throw SessionError.noPeerConnected(deviceId) }
        return companion
    }

    private func requireCompanion() throws -> BridgethingCompanion {
        guard let companion = stateLock.withLock({ self.companion }) else {
            throw SessionError.notStarted
        }
        return companion
    }

    private func parseUuid(_ id: String) throws -> UUID {
        guard let uuid = UUID(uuidString: id) else {
            throw SessionError.invalidUuid(id)
        }
        return uuid
    }

    private func unwrapVoidErr<T>(_ result: RequestResult<T, Never>, label: String) throws -> T {
        switch result {
        case let .ok(value):
            return value
        case .domain:
            // Never is uninhabited; this branch is unreachable.
            fatalError("RequestResult<_, Never>.domain is uninhabited")
        case let .protocolError(err):
            throw SessionError.protocolError(err)
        }
    }

    private func unwrapWebappErr<T>(_ result: RequestResult<T, WebappError>, label: String) throws -> T {
        switch result {
        case let .ok(value):
            return value
        case let .domain(err):
            throw SessionError.webappError(err)
        case let .protocolError(err):
            throw SessionError.protocolError(err)
        }
    }

    /// Persist webapp icon bytes and return a stable file URL.
    /// Same `(deviceId, id)` always writes the same path so the RN image cache remains valid.
    private static func writeIconToCache(deviceId: String, id: String, mime: String?, bytes: Data) throws -> URL {
        let caches = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first!
        let dir = caches.appendingPathComponent("bridgething-webapp-icons", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let ext: String = {
            switch mime {
            case "image/png": return "png"
            case "image/jpeg", "image/jpg": return "jpg"
            case "image/webp": return "webp"
            case "image/svg+xml": return "svg"
            default: return "bin"
            }
        }()
        let safeDevice = deviceId.replacingOccurrences(of: "/", with: "_")
        let safeId = id.replacingOccurrences(of: "/", with: "_")
        let url = dir.appendingPathComponent("\(safeDevice)__\(safeId).\(ext)")
        try bytes.write(to: url, options: .atomic)
        return url
    }

    // MARK: - Emit helpers

    private func emitProvider(_ info: BridgethingProviderInfo?) {
        stateLock.withLock { onProviderChanged }?(info)
    }

    private func emitAuth(_ state: BridgethingAuthState) {
        stateLock.withLock { onAuthStateChanged }?(state)
    }

    private func emitPeerConnected(_ peer: BridgethingSessionPeer) {
        stateLock.withLock { onPeerConnected }?(peer)
    }

    private func emitPeerDisconnected(_ id: String) {
        stateLock.withLock { onPeerDisconnected }?(id)
    }

    private func emitNowPlaying(_ np: BridgethingNowPlaying?) {
        stateLock.withLock { onNowPlayingChanged }?(np)
    }

    private func emitAncsAuthStatus(_ status: BridgethingAncsAuthStatus) {
        stateLock.withLock { onAncsAuthStatusChanged }?(status)
    }

    private func emitLog(_ level: String, _ message: String) {
        stateLock.withLock { onLog }?(level, message)
    }

    private func emitWebappsChanged(_ deviceId: String) {
        stateLock.withLock { onWebappsChanged }?(deviceId)
    }

    private func emitDeviceMetaChanged(_ deviceId: String, _ meta: BridgethingDeviceMeta) {
        stateLock.withLock { onDeviceMetaChanged }?(deviceId, meta)
    }

    private func emitOtaEvent(_ event: BridgethingOtaEvent) {
        stateLock.withLock { onOtaEvent }?(event)
    }

    // MARK: - Wire → RN conversion

    private static func toRNNowPlaying(_ glue: GlueNowPlaying) -> BridgethingNowPlaying {
        let item = glue.update.mediaItem
        let track: BridgethingNowPlayingTrack? = item.map { mi in
            BridgethingNowPlayingTrack(
                id: mi.persistentId,
                title: mi.title,
                artist: mi.artist,
                album: mi.album,
                artworkUrl: glue.artworkUrl,
                durationMs: mi.durationMs.map { Double($0) }
            )
        }
        let pb = glue.update.playback
        let mode: BridgethingRepeatMode = switch pb?.`repeat` ?? .off {
        case .off: .off
        case .one: .one
        case .all: .all
        }
        let playback = BridgethingNowPlayingPlayback(
            playing: pb?.playing ?? false,
            positionMs: Double(pb?.positionMs ?? 0),
            shuffle: pb?.shuffle ?? false,
            repeatMode: mode
        )
        return BridgethingNowPlaying(track: track, playback: playback, appName: pb?.appDisplayName)
    }

    private static func toRNDeviceMeta(_ meta: BridgeThingMeta) -> BridgethingDeviceMeta {
        BridgethingDeviceMeta(
            daemonVersion: meta.appVersion,
            appName: meta.appName,
            osName: meta.osName,
            osVersion: meta.osVersion,
            channel: meta.channel,
            modelName: meta.modelName,
            serialNumber: meta.serialNumber
        )
    }

    private static func toRNWebappInfo(_ info: BridgethingSchema.WebappInfo) -> BridgethingWebappInfo {
        BridgethingWebappInfo(
            id: info.id.uuidString.lowercased(),
            name: info.name,
            source: info.source == .builtin ? .builtin : .installed,
            role: info.role == .launcher ? .launcher : .standard,
            version: info.version,
            description: info.description,
            iconAvailable: info.iconAvailable,
            iconMime: info.iconMime,
            config: info.config.map(toRNConfigField),
            permissions: info.permissions
        )
    }

    private static func toRNConfigField(_ field: ConfigField) -> BridgethingConfigField {
        switch field {
        case let .string(f):
            return BridgethingConfigField(
                kind: .string,
                key: f.key,
                label: f.label,
                pattern: f.pattern,
                minLength: f.minLength.map(Double.init),
                maxLength: f.maxLength.map(Double.init),
                min: nil,
                max: nil,
                step: nil,
                choices: nil,
                defaultValue: f.default
            )
        case let .secret(f):
            return BridgethingConfigField(
                kind: .secret,
                key: f.key,
                label: f.label,
                pattern: f.pattern,
                minLength: f.minLength.map(Double.init),
                maxLength: f.maxLength.map(Double.init),
                min: nil,
                max: nil,
                step: nil,
                choices: nil,
                defaultValue: f.default
            )
        case let .number(f):
            return BridgethingConfigField(
                kind: .number,
                key: f.key,
                label: f.label,
                pattern: nil,
                minLength: nil,
                maxLength: nil,
                min: f.min,
                max: f.max,
                step: f.step,
                choices: nil,
                defaultValue: f.default.map { "\($0)" }
            )
        case let .boolean(f):
            return BridgethingConfigField(
                kind: .boolean,
                key: f.key,
                label: f.label,
                pattern: nil,
                minLength: nil,
                maxLength: nil,
                min: nil,
                max: nil,
                step: nil,
                choices: nil,
                defaultValue: f.default.map { $0 ? "true" : "false" }
            )
        case let .enum(f):
            return BridgethingConfigField(
                kind: .enum,
                key: f.key,
                label: f.label,
                pattern: nil,
                minLength: nil,
                maxLength: nil,
                min: nil,
                max: nil,
                step: nil,
                choices: f.choices,
                defaultValue: f.default
            )
        }
    }
}

private enum SessionError: Error {
    case deallocated
    case cancelled
    case notStarted
    case unknownProvider(String)
    case noPeerConnected(String)
    case invalidUuid(String)
    case invalidArchive
    case installInterrupted
    case installTimedOut
    case webappError(WebappError)
    case protocolError(WireError)
    case unsupportedOnPlatform
}

private enum InstallOutcome {
    case installed(WebappInfo)
    case failed(WebappError)
}

private extension BridgethingAuthState {
    static func idleState() -> BridgethingAuthState {
        .init(kind: .idle, userCode: nil, verificationUrl: nil, verificationUrlComplete: nil, message: nil)
    }
    static func pendingState(userCode: String?, verificationUrl: String?, verificationUrlComplete: String?) -> BridgethingAuthState {
        .init(kind: .pending, userCode: userCode, verificationUrl: verificationUrl, verificationUrlComplete: verificationUrlComplete, message: nil)
    }
    static func authenticated() -> BridgethingAuthState {
        .init(kind: .authenticated, userCode: nil, verificationUrl: nil, verificationUrlComplete: nil, message: nil)
    }
    static func failed(message: String) -> BridgethingAuthState {
        .init(kind: .failed, userCode: nil, verificationUrl: nil, verificationUrlComplete: nil, message: message)
    }
}

private extension NSLock {
    @discardableResult
    func withLock<T>(_ body: () throws -> T) rethrows -> T {
        lock(); defer { unlock() }
        return try body()
    }
}

/// Wire enum -> RN string union.
private func toRNAncsAuthStatus(_ state: AncsAuthState) -> BridgethingAncsAuthStatus {
    switch state {
    case .unknown: .unknown
    case .probing: .probing
    case .authorized: .authorized
    case .unauthorized: .unauthorized
    }
}

private func toRNAncsSetupResult(_ result: AncsSetupResult) -> BridgethingAncsSetupResult {
    let (kind, message): (BridgethingAncsSetupKind, String?) = switch result.kind {
    case .paired: (.paired, nil)
    case .alreadyPaired: (.alreadypaired, nil)
    case .cancelled: (.cancelled, nil)
    case .unsupported: (.unsupported, nil)
    case let .failed(reason): (.failed, reason)
    }
    return BridgethingAncsSetupResult(
        kind: kind,
        authStatus: toRNAncsAuthStatus(result.authState),
        message: message
    )
}

private func toRNOtaEvent(_ event: OtaPollEvent) -> BridgethingOtaEvent {
    switch event {
    case let .manifestPolled(updatedAt):
        return BridgethingOtaEvent(
            kind: .manifestpolled,
            updatedAt: updatedAt,
            reason: nil, deviceId: nil, otaKind: nil,
            fromVersion: nil, toVersion: nil, phase: nil, percent: nil,
            deviceChannel: nil, configuredChannel: nil
        )
    case let .manifestPollFailed(reason):
        return BridgethingOtaEvent(
            kind: .manifestpollfailed,
            updatedAt: nil, reason: reason, deviceId: nil, otaKind: nil,
            fromVersion: nil, toVersion: nil, phase: nil, percent: nil,
            deviceChannel: nil, configuredChannel: nil
        )
    case let .channelMismatch(deviceId, deviceChannel, configuredChannel):
        return BridgethingOtaEvent(
            kind: .channelmismatch,
            updatedAt: nil,
            reason: "device on \(deviceChannel), companion configured for \(configuredChannel)",
            deviceId: deviceId, otaKind: nil,
            fromVersion: nil, toVersion: nil, phase: nil, percent: nil,
            deviceChannel: deviceChannel, configuredChannel: configuredChannel
        )
    case let .updateAvailable(deviceId, kind, fromVersion, toVersion):
        return BridgethingOtaEvent(
            kind: .updateavailable,
            updatedAt: nil, reason: nil, deviceId: deviceId,
            otaKind: kind == .image ? .image : .daemon,
            fromVersion: fromVersion, toVersion: toVersion,
            phase: nil, percent: nil,
            deviceChannel: nil, configuredChannel: nil
        )
    case let .progress(deviceId, kind, snapshot):
        let (phase, percent, reason): (BridgethingOtaPhase, Double, String?) = {
            switch snapshot {
            case .idle: return (.idle, 0, nil)
            case let .streaming(p): return (.streaming, Double(p), nil)
            case let .applying(phase: ph, percent: p):
                let mapped: BridgethingOtaPhase = switch ph {
                case .streaming: .streaming
                case .verifying: .verifying
                case .writing: .writing
                case .confirming: .confirming
                case .reboot: .reboot
                }
                return (mapped, Double(p), nil)
            case .completed: return (.completed, 100, nil)
            case let .failed(r): return (.failed, 0, r)
            }
        }()
        return BridgethingOtaEvent(
            kind: .progress,
            updatedAt: nil, reason: reason, deviceId: deviceId,
            otaKind: kind == .image ? .image : .daemon,
            fromVersion: nil, toVersion: nil, phase: phase, percent: percent,
            deviceChannel: nil, configuredChannel: nil
        )
    case let .updated(deviceId, kind, version):
        return BridgethingOtaEvent(
            kind: .updated,
            updatedAt: nil, reason: nil, deviceId: deviceId,
            otaKind: kind == .image ? .image : .daemon,
            fromVersion: nil, toVersion: version, phase: nil, percent: nil,
            deviceChannel: nil, configuredChannel: nil
        )
    case let .failed(deviceId, kind, reason):
        return BridgethingOtaEvent(
            kind: .failed,
            updatedAt: nil, reason: reason, deviceId: deviceId,
            otaKind: kind == .image ? .image : .daemon,
            fromVersion: nil, toVersion: nil, phase: nil, percent: nil,
            deviceChannel: nil, configuredChannel: nil
        )
    }
}
