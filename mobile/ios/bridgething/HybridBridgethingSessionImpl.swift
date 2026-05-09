import BridgethingCompanion
import BridgethingGateway
import BridgethingGlue
import BridgethingLyrics
import BridgethingSchema
import BridgethingSession
import Foundation
import SafariServices
import UIKit

/// Real `BridgethingSessionBackend` impl for the bridgething host app.
/// Owns one `BridgethingCompanion` (which owns the gateway, the active
/// glue, and every dispatcher).
///
/// Glue registration happens before the backend is installed: the
/// `BridgethingApp` setup code populates the static `registry` with a
/// `ProviderRegistration` per provider id. Each registration carries a
/// factory closure (taking a `BackendContext` so the glue's
/// authenticator can publish device-code prompts back to RN as
/// `BridgethingAuthState` updates) and a `signOut` closure that clears
/// the host's persisted credentials.
public final class HybridBridgethingSessionImpl: BridgethingSessionBackend, @unchecked Sendable {
    public typealias GlueFactory = @Sendable (BackendContext) -> any BridgethingGlue
    public typealias SignOutFn = @Sendable () -> Void

    public struct BackendContext: Sendable {
        public let emitAuth: @Sendable (BridgethingAuthState) -> Void
    }

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

    public init() {}

    // MARK: - Lifecycle

    public func start() async throws {
        let adapter = EAAccessoryAdapter(protocolString: Self.eaProtocolString)
        let companion = BridgethingCompanion(
            adapter: adapter,
            lyricsResolver: Self.lyricsResolver,
            host: Self.hostInfo,
            capabilities: CapabilityFlagsStore.load()
        )
        stateLock.lock(); self.companion = companion; stateLock.unlock()

        await companion.setNowPlayingObserver { [weak self] np in
            self?.handleNowPlaying(np)
        }
        await companion.setAncsAuthStateObserver { [weak self] state in
            self?.emitAncsAuthStatus(toRNAncsAuthStatus(state))
        }

        try await companion.start()

        // Restore any previously-saved OTA poll config.
        let ota = await companion.ota
        if let storedPoll = OtaPollConfigStore.load() {
            await ota.setPollConfig(storedPoll)
        }

        let events = companion.gateway.events
        let task = Task { [weak self] in
            for await event in events {
                self?.handleGatewayEvent(event)
            }
        }
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

        await dismissPresentedSafari()
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
                await dismissPresentedSafari()
            }
            stateLock.lock(); authTask = task; stateLock.unlock()
        }
    }

    public func cancelAuth() async {
        stateLock.lock(); let task = authTask; stateLock.unlock()
        task?.cancel()
        await dismissPresentedSafari()
        let companion = stateLock.withLock { self.companion }
        try? await companion?.setActive(nil)
        emitProvider(nil)
        emitAuth(.idleState())
    }

    public func signOut() async {
        stateLock.lock(); let task = authTask; stateLock.unlock()
        task?.cancel()
        await dismissPresentedSafari()

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

    // MARK: - Device naming

    public func setDeviceNickname(deviceId: String, nickname: String?) async {
        let trimmed = nickname?.trimmingCharacters(in: .whitespacesAndNewlines)
        let normalized: String? = (trimmed?.isEmpty ?? true) ? nil : trimmed
        DeviceNicknameStore.set(deviceId: deviceId, nickname: normalized)
        // Re-emit the peer with the merged nickname so RN re-renders.
        let updated: BridgethingSessionPeer? = stateLock.withLock {
            guard var peer = peers[deviceId] else { return nil }
            peer = BridgethingSessionPeer(id: peer.id, name: peer.name, nickname: normalized)
            peers[deviceId] = peer
            return peer
        }
        if let updated {
            emitPeerConnected(updated)
        }
    }

    public func getDeviceNickname(deviceId: String) async -> String? {
        DeviceNicknameStore.get(deviceId: deviceId)
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

    public func installWebappFromUrl(deviceId: String, url: String) async throws -> BridgethingWebappInfo {
        guard let parsed = URL(string: url) else {
            throw SessionError.invalidUrl(url)
        }
        let (data, response) = try await URLSession.shared.data(from: parsed)
        if let http = response as? HTTPURLResponse, !(200 ..< 300).contains(http.statusCode) {
            throw SessionError.installDownloadFailed(status: http.statusCode)
        }
        let companion = try requirePeerConnected(deviceId)
        let req = WebappInstall(archive: data)
        let result = try await companion.gateway.webapp.install(deviceId: deviceId, req)
        let info = try unwrapWebappErr(result, label: "installWebapp")
        emitWebappsChanged(deviceId)
        return Self.toRNWebappInfo(info)
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
            return BridgethingWebappIcon(
                base64: reply.bytes.base64EncodedString(),
                mime: reply.mime
            )
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

    public func getCapabilityFlags() async -> BridgethingCapabilityFlags {
        let flags = CapabilityFlagsStore.load()
        return BridgethingCapabilityFlags(
            geo: flags.geo,
            notifications: flags.notifications,
            netFetch: flags.netFetch,
            netWs: flags.netWs,
            audioTts: flags.audioTts
        )
    }

    public func setCapabilityFlags(flags: BridgethingCapabilityFlags) async {
        let companionFlags = CompanionCapabilityFlags(
            geo: flags.geo,
            notifications: flags.notifications,
            netFetch: flags.netFetch,
            netWs: flags.netWs,
            audioTts: flags.audioTts
        )
        CapabilityFlagsStore.save(companionFlags)
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
            OtaPollConfigStore.save(mapped)
            await ota?.setPollConfig(mapped)
        } else {
            OtaPollConfigStore.clear()
            await ota?.setPollConfig(nil)
        }
    }

    public func getOtaPollConfig() async -> BridgethingOtaPollConfig? {
        guard let stored = OtaPollConfigStore.load() else { return nil }
        return BridgethingOtaPollConfig(
            channel: stored.channel,
            intervalSeconds: stored.intervalSeconds,
            autoPush: stored.autoPush,
            rootUrl: stored.rootURL.absoluteString
        )
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

    public func setOnWebappsChanged(_ callback: @escaping @Sendable (String) -> Void) {
        stateLock.withLock { onWebappsChanged = callback }
    }

    public func setOnDeviceMetaChanged(_ callback: @escaping @Sendable (String, BridgethingDeviceMeta) -> Void) {
        stateLock.withLock { onDeviceMetaChanged = callback }
    }

    public func setOnOtaEvent(_ callback: @escaping @Sendable (BridgethingOtaEvent) -> Void) {
        stateLock.withLock { onOtaEvent = callback }
    }

    // MARK: - Internal

    private func runSetActive(id: String?) async throws {
        let companion = stateLock.withLock { self.companion }
        guard let companion else { throw SessionError.notStarted }

        if let id {
            guard let registration = Self.registry.first(where: { $0.id == id }) else {
                throw SessionError.unknownProvider(id)
            }
            emitAuth(.pendingState(userCode: nil, verificationUrl: nil, verificationUrlComplete: nil))

            let context = BackendContext(emitAuth: { [weak self] state in
                self?.handleAuthFromGlue(state)
            })
            let glue = registration.factory(context)
            try await companion.setActive(glue)

            try Task.checkCancellation()
            emitProvider(BridgethingProviderInfo(
                id: registration.id,
                displayName: registration.displayName,
                available: registration.available
            ))
            emitAuth(.authenticated())
        } else {
            try await companion.setActive(nil)
            emitProvider(nil)
            emitAuth(.idleState())
        }
    }

    private func handleAuthFromGlue(_ state: BridgethingAuthState) {
        emitAuth(state)
        if state.kind == .pending,
           let urlString = state.verificationUrlComplete,
           let url = URL(string: urlString)
        {
            Task { await Self.presentSafari(url) }
        }
    }

    private func handleGatewayEvent(_ event: GatewayEvent) {
        switch event {
        case let .connected(device):
            let nickname = DeviceNicknameStore.get(deviceId: device.id)
            let peer = BridgethingSessionPeer(
                id: device.id,
                name: device.name,
                nickname: nickname
            )
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

    // MARK: - SFSafariViewController plumbing

    @MainActor private static weak var presentedSafari: SFSafariViewController?

    fileprivate static func presentSafari(_ url: URL) async {
        await MainActor.run {
            if let existing = presentedSafari {
                existing.dismiss(animated: false)
                presentedSafari = nil
            }
            guard let root = keyRootViewController() else { return }
            let safari = SFSafariViewController(url: url)
            safari.modalPresentationStyle = .formSheet
            presentedSafari = safari
            root.present(safari, animated: true)
        }
    }

    private func dismissPresentedSafari() async {
        await MainActor.run {
            Self.presentedSafari?.dismiss(animated: true)
            Self.presentedSafari = nil
        }
    }

    @MainActor private static func keyRootViewController() -> UIViewController? {
        UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .flatMap(\.windows)
            .first { $0.isKeyWindow }?
            .rootViewController
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

// MARK: - Persisted-defaults helpers

private enum DeviceNicknameStore {
    private static let defaults = UserDefaults.standard
    private static let key = "dev.bridgething.deviceNicknames"

    static func get(deviceId: String) -> String? {
        load()[deviceId]
    }

    static func set(deviceId: String, nickname: String?) {
        var map = load()
        if let nickname { map[deviceId] = nickname } else { map.removeValue(forKey: deviceId) }
        if let data = try? JSONEncoder().encode(map) {
            defaults.set(data, forKey: key)
        }
    }

    private static func load() -> [String: String] {
        guard let data = defaults.data(forKey: key),
              let map = try? JSONDecoder().decode([String: String].self, from: data)
        else { return [:] }
        return map
    }
}

private enum CapabilityFlagsStore {
    private static let defaults = UserDefaults.standard
    private static let key = "dev.bridgething.capabilityFlags"

    static func load() -> CompanionCapabilityFlags {
        guard let data = defaults.data(forKey: key),
              let stored = try? JSONDecoder().decode(StoredFlags.self, from: data)
        else {
            return CompanionCapabilityFlags()
        }
        return CompanionCapabilityFlags(
            geo: stored.geo,
            notifications: stored.notifications,
            netFetch: stored.netFetch,
            netWs: stored.netWs,
            audioTts: stored.audioTts
        )
    }

    static func save(_ flags: CompanionCapabilityFlags) {
        let stored = StoredFlags(
            geo: flags.geo,
            notifications: flags.notifications,
            netFetch: flags.netFetch,
            netWs: flags.netWs,
            audioTts: flags.audioTts
        )
        if let data = try? JSONEncoder().encode(stored) {
            defaults.set(data, forKey: key)
        }
    }

    private struct StoredFlags: Codable {
        let geo: Bool
        let notifications: Bool
        let netFetch: Bool
        let netWs: Bool
        let audioTts: Bool
    }
}

private enum OtaPollConfigStore {
    private static let defaults = UserDefaults.standard
    private static let key = "dev.bridgething.otaPollConfig"

    static func load() -> OtaPollConfig? {
        guard let data = defaults.data(forKey: key),
              let stored = try? JSONDecoder().decode(StoredConfig.self, from: data),
              let url = URL(string: stored.rootUrl)
        else {
            return nil
        }
        return OtaPollConfig(
            rootURL: url,
            channel: stored.channel,
            intervalSeconds: stored.intervalSeconds,
            cacheDirectory: nil,
            autoPush: stored.autoPush
        )
    }

    static func save(_ config: OtaPollConfig) {
        let stored = StoredConfig(
            channel: config.channel,
            intervalSeconds: config.intervalSeconds,
            autoPush: config.autoPush,
            rootUrl: config.rootURL.absoluteString
        )
        if let data = try? JSONEncoder().encode(stored) {
            defaults.set(data, forKey: key)
        }
    }

    static func clear() {
        defaults.removeObject(forKey: key)
    }

    private struct StoredConfig: Codable {
        let channel: String
        let intervalSeconds: TimeInterval
        let autoPush: Bool
        let rootUrl: String
    }
}

private enum SessionError: Error {
    case deallocated
    case cancelled
    case notStarted
    case unknownProvider(String)
    case noPeerConnected(String)
    case invalidUrl(String)
    case invalidUuid(String)
    case installDownloadFailed(status: Int)
    case webappError(WebappError)
    case protocolError(WireError)
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

/// Wire enum → RN string union.
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
