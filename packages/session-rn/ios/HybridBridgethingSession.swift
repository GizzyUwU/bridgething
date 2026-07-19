import Foundation
import NitroModules

/// backend protocol the host app implements; decouples the Nitro HybridObject from host-app orchestration logic.
public protocol BridgethingSessionBackend: AnyObject, Sendable {
    func start() async throws
    func stop() async

    func availableProviders() async -> [BridgethingProviderInfo]
    func setActiveProvider(id: String?) async throws
    func cancelAuth() async
    func signOut() async
    func currentProvider() async -> BridgethingProviderInfo?

    func snapshot() async -> BridgethingSessionSnapshot
    func deviceLogSnapshot(limit: Double) async -> [BridgethingDeviceLogLine]
    func companionDebug() async -> BridgethingCompanionDebug

    func persistedLogSize() async -> Double
    func exportLogs() async throws -> String
    func shareLogs() async -> Bool
    func clearPersistedLogs() async

    func enableAncsNotifications() async -> BridgethingAncsSetupResult
    func ancsAuthStatus() async -> BridgethingAncsAuthStatus

    func listWebapps(deviceId: String) async throws -> [BridgethingWebappInfo]
    func currentWebapp(deviceId: String) async throws -> BridgethingActiveWebapp?
    func installWebapp(deviceId: String, sourceUri: String) async throws -> BridgethingWebappInfo
    func uninstallWebapp(deviceId: String, id: String) async throws
    func switchWebapp(deviceId: String, id: String) async throws
    func webappIcon(deviceId: String, id: String) async throws -> BridgethingWebappIcon?
    func webappSettingsPage(deviceId: String, id: String) async throws -> String
    func listWebappConfig(deviceId: String, id: String) async throws -> [BridgethingConfigEntry]
    func setWebappConfigField(deviceId: String, id: String, key: String, value: String) async throws
    func deleteWebappConfigField(deviceId: String, id: String, key: String) async throws
    func getWebappDoc(deviceId: String, id: String, key: String) async throws -> String?
    func listWebappDoc(deviceId: String, id: String) async throws -> [BridgethingDocEntry]
    func setWebappDoc(deviceId: String, id: String, key: String, value: String) async throws
    func deleteWebappDoc(deviceId: String, id: String, key: String) async throws

    func setCapabilityFlags(flags: BridgethingCapabilityFlags) async

    func setDeviceAutoResume(deviceId: String, enabled: Bool) async
    func isDeviceAutoResumeEnabled(deviceId: String) async -> Bool

    func setOtaPollConfig(config: BridgethingOtaPollConfig?) async
    func checkForOtaUpdate(rootUrl: String?) async
    func fetchOtaManifest(rootUrl: String?) async throws -> BridgethingOtaManifest
    func applyOtaUpdate(deviceId: String, channel: String, version: String, rootUrl: String?) async throws

    func catalogSources() async -> [String]
    func addCatalogSource(url: String) async
    func removeCatalogSource(url: String) async
    func refreshCatalog() async
    func availableCatalogApps(deviceId: String) async -> String
    func checkForCatalogUpdates(deviceId: String) async -> String
    func installCatalogApp(deviceId: String, appId: String, version: String, sourceUrl: String) async throws -> BridgethingWebappInfo
    func setCatalogPollConfig(config: BridgethingCatalogPollConfig?) async

    func reconnectPeer(deviceId: String) async throws

    func deviceSetNickname(deviceId: String, nickname: String) async throws

    func presentPairPicker() async throws -> BridgethingBtDevice?

    func isNotificationAccessGranted() async -> Bool
    func requestNotificationAccess() async throws

    func isDefaultDialer() async -> Bool
    func requestDefaultDialer() async throws

    func forgetCompanionDevice(mac: String) async throws

    func isIgnoringBatteryOptimizations() async -> Bool
    func requestIgnoreBatteryOptimizations() async throws

    func revokeRuntimePermissions(permissions: [String]) async -> Bool
    func killApp() async

    func setOnProviderChanged(_ callback: @escaping @Sendable (BridgethingProviderInfo?) -> Void)
    func setOnAuthStateChanged(_ callback: @escaping @Sendable (BridgethingAuthState) -> Void)
    func setOnServiceHealthChanged(_ callback: @escaping @Sendable (BridgethingServiceHealth) -> Void)
    func setOnPeerConnected(_ callback: @escaping @Sendable (BridgethingSessionPeer) -> Void)
    func setOnPeerDisconnected(_ callback: @escaping @Sendable (String) -> Void)
    func setOnPeerLinkFailed(_ callback: @escaping @Sendable (BridgethingSessionPeer) -> Void)
    func setOnNowPlayingChanged(_ callback: @escaping @Sendable (BridgethingNowPlaying?) -> Void)
    func setOnAncsAuthStatusChanged(_ callback: @escaping @Sendable (BridgethingAncsAuthStatus) -> Void)
    func setOnLog(_ callback: @escaping @Sendable (String, String) -> Void)
    func setLogStreamingEnabled(_ enabled: Bool)
    func setLocalLogStreamingEnabled(_ enabled: Bool)

    func setOnWebappsChanged(_ callback: @escaping @Sendable (String) -> Void)
    func setOnWebappDocChanged(_ callback: @escaping @Sendable (String, String, String, String?) -> Void)
    func setOnDeviceMetaChanged(_ callback: @escaping @Sendable (String, BridgethingDeviceMeta) -> Void)
    func setOnOtaEvent(_ callback: @escaping @Sendable (BridgethingOtaEvent) -> Void)
    func setOnCatalogEvent(_ callback: @escaping @Sendable (BridgethingCatalogEvent) -> Void)
}

/// thin Nitro proxy; buffers callback setters until a backend is installed via `installBackend(_:)`.
public final class HybridBridgethingSession: HybridBridgethingSessionSpec, @unchecked Sendable {
    private static let stateLock = NSLock()
    private static var _backend: (any BridgethingSessionBackend)?

    private static var pendingProviderChanged: (@Sendable (BridgethingProviderInfo?) -> Void)?
    private static var pendingAuthStateChanged: (@Sendable (BridgethingAuthState) -> Void)?
    private static var pendingServiceHealthChanged: (@Sendable (BridgethingServiceHealth) -> Void)?
    private static var pendingPeerConnected: (@Sendable (BridgethingSessionPeer) -> Void)?
    private static var pendingPeerDisconnected: (@Sendable (String) -> Void)?
    private static var pendingPeerLinkFailed: (@Sendable (BridgethingSessionPeer) -> Void)?
    private static var pendingNowPlayingChanged: (@Sendable (BridgethingNowPlaying?) -> Void)?
    private static var pendingAncsAuthStatusChanged: (@Sendable (BridgethingAncsAuthStatus) -> Void)?
    private static var pendingLog: (@Sendable (String, String) -> Void)?
    private static var pendingWebappsChanged: (@Sendable (String) -> Void)?
    private static var pendingWebappDocChanged: (@Sendable (String, String, String, String?) -> Void)?
    private static var pendingDeviceMetaChanged: (@Sendable (String, BridgethingDeviceMeta) -> Void)?
    private static var pendingOtaEvent: (@Sendable (BridgethingOtaEvent) -> Void)?
    private static var pendingCatalogEvent: (@Sendable (BridgethingCatalogEvent) -> Void)?

    /// install the backend; must be called before RN starts. replays any already-registered callback setters.
    public static func installBackend(_ backend: any BridgethingSessionBackend) {
        stateLock.lock()
        _backend = backend
        let providerCb = pendingProviderChanged
        let authCb = pendingAuthStateChanged
        let healthCb = pendingServiceHealthChanged
        let peerConnCb = pendingPeerConnected
        let peerDisconnCb = pendingPeerDisconnected
        let peerLinkFailedCb = pendingPeerLinkFailed
        let nowPlayingCb = pendingNowPlayingChanged
        let ancsCb = pendingAncsAuthStatusChanged
        let logCb = pendingLog
        let webappsCb = pendingWebappsChanged
        let webappDocCb = pendingWebappDocChanged
        let deviceMetaCb = pendingDeviceMetaChanged
        let otaCb = pendingOtaEvent
        let catalogCb = pendingCatalogEvent
        pendingProviderChanged = nil
        pendingAuthStateChanged = nil
        pendingServiceHealthChanged = nil
        pendingPeerConnected = nil
        pendingPeerDisconnected = nil
        pendingPeerLinkFailed = nil
        pendingNowPlayingChanged = nil
        pendingAncsAuthStatusChanged = nil
        pendingLog = nil
        pendingWebappsChanged = nil
        pendingWebappDocChanged = nil
        pendingDeviceMetaChanged = nil
        pendingOtaEvent = nil
        pendingCatalogEvent = nil
        stateLock.unlock()

        if let providerCb { backend.setOnProviderChanged(providerCb) }
        if let authCb { backend.setOnAuthStateChanged(authCb) }
        if let healthCb { backend.setOnServiceHealthChanged(healthCb) }
        if let peerConnCb { backend.setOnPeerConnected(peerConnCb) }
        if let peerDisconnCb { backend.setOnPeerDisconnected(peerDisconnCb) }
        if let peerLinkFailedCb { backend.setOnPeerLinkFailed(peerLinkFailedCb) }
        if let nowPlayingCb { backend.setOnNowPlayingChanged(nowPlayingCb) }
        if let ancsCb { backend.setOnAncsAuthStatusChanged(ancsCb) }
        if let logCb { backend.setOnLog(logCb) }
        if let webappsCb { backend.setOnWebappsChanged(webappsCb) }
        if let webappDocCb { backend.setOnWebappDocChanged(webappDocCb) }
        if let deviceMetaCb { backend.setOnDeviceMetaChanged(deviceMetaCb) }
        if let otaCb { backend.setOnOtaEvent(otaCb) }
        if let catalogCb { backend.setOnCatalogEvent(catalogCb) }
    }

    private static func backend() throws -> any BridgethingSessionBackend {
        stateLock.lock(); defer { stateLock.unlock() }
        guard let b = _backend else {
            throw RuntimeError.error(withMessage: "BridgethingSession backend not installed - host app must call HybridBridgethingSession.installBackend(_:) before React Native starts")
        }
        return b
    }

    private static func unwrapString(_ variant: Variant_NullType_String?) -> String? {
        variant.flatMap { v in
            switch v {
            case .first: nil
            case let .second(value): value
            }
        }
    }

    override public init() { super.init() }

    // MARK: - Lifecycle

    public func start() throws -> Promise<Void> {
        Promise.async {
            try await Self.backend().start()
        }
    }

    public func stop() throws -> Promise<Void> {
        Promise.async {
            await (try Self.backend()).stop()
        }
    }

    // MARK: - Provider selection

    public func availableProviders() throws -> Promise<[BridgethingProviderInfo]> {
        Promise.async {
            await (try Self.backend()).availableProviders()
        }
    }

    public func setActiveProvider(id: Variant_NullType_String?) throws -> Promise<Void> {
        let stringId: String? = id.flatMap { variant in
            switch variant {
            case .first: nil
            case let .second(value): value
            }
        }
        return Promise.async {
            try await Self.backend().setActiveProvider(id: stringId)
        }
    }

    public func cancelAuth() throws -> Promise<Void> {
        Promise.async {
            await (try Self.backend()).cancelAuth()
        }
    }

    public func signOut() throws -> Promise<Void> {
        Promise.async {
            await (try Self.backend()).signOut()
        }
    }

    public func currentProvider() throws -> Promise<Variant_NullType_BridgethingProviderInfo> {
        Promise.async {
            let info = await (try Self.backend()).currentProvider()
            return info.map { .second($0) } ?? .first(NullType.null)
        }
    }

    public func snapshot() throws -> Promise<BridgethingSessionSnapshot> {
        Promise.async {
            await (try Self.backend()).snapshot()
        }
    }

    public func deviceLogSnapshot(limit: Double) throws -> Promise<[BridgethingDeviceLogLine]> {
        Promise.async {
            await (try Self.backend()).deviceLogSnapshot(limit: limit)
        }
    }

    public func companionDebug() throws -> Promise<BridgethingCompanionDebug> {
        Promise.async {
            await (try Self.backend()).companionDebug()
        }
    }

    public func persistedLogSize() throws -> Promise<Double> {
        Promise.async {
            await (try Self.backend()).persistedLogSize()
        }
    }

    public func exportLogs() throws -> Promise<String> {
        Promise.async {
            try await (try Self.backend()).exportLogs()
        }
    }

    public func shareLogs() throws -> Promise<Bool> {
        Promise.async {
            await (try Self.backend()).shareLogs()
        }
    }

    public func clearPersistedLogs() throws -> Promise<Void> {
        Promise.async {
            await (try Self.backend()).clearPersistedLogs()
        }
    }

    // MARK: - ANCS

    public func enableAncsNotifications() throws -> Promise<BridgethingAncsSetupResult> {
        Promise.async {
            await (try Self.backend()).enableAncsNotifications()
        }
    }

    public func ancsAuthStatus() throws -> Promise<BridgethingAncsAuthStatus> {
        Promise.async {
            await (try Self.backend()).ancsAuthStatus()
        }
    }

    // MARK: - Webapps (per-device)

    public func listWebapps(deviceId: String) throws -> Promise<[BridgethingWebappInfo]> {
        Promise.async {
            try await Self.backend().listWebapps(deviceId: deviceId)
        }
    }

    public func currentWebapp(deviceId: String) throws -> Promise<Variant_NullType_BridgethingActiveWebapp> {
        Promise.async {
            let active = try await Self.backend().currentWebapp(deviceId: deviceId)
            return active.map { .second($0) } ?? .first(NullType.null)
        }
    }

    public func installWebapp(deviceId: String, sourceUri: String) throws -> Promise<BridgethingWebappInfo> {
        Promise.async {
            try await Self.backend().installWebapp(deviceId: deviceId, sourceUri: sourceUri)
        }
    }

    public func uninstallWebapp(deviceId: String, id: String) throws -> Promise<Void> {
        Promise.async {
            try await Self.backend().uninstallWebapp(deviceId: deviceId, id: id)
        }
    }

    public func switchWebapp(deviceId: String, id: String) throws -> Promise<Void> {
        Promise.async {
            try await Self.backend().switchWebapp(deviceId: deviceId, id: id)
        }
    }

    public func webappIcon(deviceId: String, id: String) throws -> Promise<Variant_NullType_BridgethingWebappIcon> {
        Promise.async {
            let icon = try await Self.backend().webappIcon(deviceId: deviceId, id: id)
            return icon.map { .second($0) } ?? .first(NullType.null)
        }
    }

    public func webappSettingsPage(deviceId: String, id: String) throws -> Promise<String> {
        Promise.async {
            try await Self.backend().webappSettingsPage(deviceId: deviceId, id: id)
        }
    }

    public func listWebappConfig(deviceId: String, id: String) throws -> Promise<[BridgethingConfigEntry]> {
        Promise.async {
            try await Self.backend().listWebappConfig(deviceId: deviceId, id: id)
        }
    }

    public func setWebappConfigField(deviceId: String, id: String, key: String, value: String) throws -> Promise<Void> {
        Promise.async {
            try await Self.backend().setWebappConfigField(deviceId: deviceId, id: id, key: key, value: value)
        }
    }

    public func deleteWebappConfigField(deviceId: String, id: String, key: String) throws -> Promise<Void> {
        Promise.async {
            try await Self.backend().deleteWebappConfigField(deviceId: deviceId, id: id, key: key)
        }
    }

    public func getWebappDoc(deviceId: String, id: String, key: String) throws -> Promise<Variant_NullType_String> {
        Promise.async {
            let value = try await Self.backend().getWebappDoc(deviceId: deviceId, id: id, key: key)
            return value.map { .second($0) } ?? .first(NullType.null)
        }
    }

    public func listWebappDoc(deviceId: String, id: String) throws -> Promise<[BridgethingDocEntry]> {
        Promise.async {
            try await Self.backend().listWebappDoc(deviceId: deviceId, id: id)
        }
    }

    public func setWebappDoc(deviceId: String, id: String, key: String, value: String) throws -> Promise<Void> {
        Promise.async {
            try await Self.backend().setWebappDoc(deviceId: deviceId, id: id, key: key, value: value)
        }
    }

    public func deleteWebappDoc(deviceId: String, id: String, key: String) throws -> Promise<Void> {
        Promise.async {
            try await Self.backend().deleteWebappDoc(deviceId: deviceId, id: id, key: key)
        }
    }

    // MARK: - Capability flags

    public func setCapabilityFlags(flags: BridgethingCapabilityFlags) throws -> Promise<Void> {
        Promise.async {
            await (try Self.backend()).setCapabilityFlags(flags: flags)
        }
    }

    // MARK: - OTA

    public func setDeviceAutoResume(deviceId: String, enabled: Bool) throws -> Promise<Void> {
        Promise.async {
            await (try Self.backend()).setDeviceAutoResume(deviceId: deviceId, enabled: enabled)
        }
    }

    public func isDeviceAutoResumeEnabled(deviceId: String) throws -> Promise<Bool> {
        Promise.async {
            await (try Self.backend()).isDeviceAutoResumeEnabled(deviceId: deviceId)
        }
    }

    public func setOtaPollConfig(config: Variant_NullType_BridgethingOtaPollConfig?) throws -> Promise<Void> {
        let unwrapped: BridgethingOtaPollConfig? = config.flatMap { variant in
            switch variant {
            case .first: nil
            case let .second(value): value
            }
        }
        return Promise.async {
            await (try Self.backend()).setOtaPollConfig(config: unwrapped)
        }
    }

    public func checkForOtaUpdate(rootUrl: Variant_NullType_String?) throws -> Promise<Void> {
        let url = Self.unwrapString(rootUrl)
        return Promise.async {
            await (try Self.backend()).checkForOtaUpdate(rootUrl: url)
        }
    }

    public func fetchOtaManifest(rootUrl: Variant_NullType_String?) throws -> Promise<BridgethingOtaManifest> {
        let url = Self.unwrapString(rootUrl)
        return Promise.async {
            try await Self.backend().fetchOtaManifest(rootUrl: url)
        }
    }

    public func applyOtaUpdate(deviceId: String, channel: String, version: String, rootUrl: Variant_NullType_String?) throws -> Promise<Void> {
        let url = Self.unwrapString(rootUrl)
        return Promise.async {
            try await Self.backend().applyOtaUpdate(deviceId: deviceId, channel: channel, version: version, rootUrl: url)
        }
    }

    // MARK: - Catalog

    public func catalogSources() throws -> Promise<[String]> {
        Promise.async { await (try Self.backend()).catalogSources() }
    }

    public func addCatalogSource(url: String) throws -> Promise<Void> {
        Promise.async { await (try Self.backend()).addCatalogSource(url: url) }
    }

    public func removeCatalogSource(url: String) throws -> Promise<Void> {
        Promise.async { await (try Self.backend()).removeCatalogSource(url: url) }
    }

    public func refreshCatalog() throws -> Promise<Void> {
        Promise.async { await (try Self.backend()).refreshCatalog() }
    }

    public func availableCatalogApps(deviceId: String) throws -> Promise<String> {
        Promise.async { await (try Self.backend()).availableCatalogApps(deviceId: deviceId) }
    }

    public func checkForCatalogUpdates(deviceId: String) throws -> Promise<String> {
        Promise.async { await (try Self.backend()).checkForCatalogUpdates(deviceId: deviceId) }
    }

    public func installCatalogApp(deviceId: String, appId: String, version: String, sourceUrl: String) throws -> Promise<BridgethingWebappInfo> {
        Promise.async {
            try await Self.backend().installCatalogApp(deviceId: deviceId, appId: appId, version: version, sourceUrl: sourceUrl)
        }
    }

    public func setCatalogPollConfig(config: Variant_NullType_BridgethingCatalogPollConfig?) throws -> Promise<Void> {
        let unwrapped: BridgethingCatalogPollConfig? = config.flatMap { variant in
            switch variant {
            case .first: nil
            case let .second(value): value
            }
        }
        return Promise.async {
            await (try Self.backend()).setCatalogPollConfig(config: unwrapped)
        }
    }

    // MARK: - Peer reconnect

    public func reconnectPeer(deviceId: String) throws -> Promise<Void> {
        Promise.async {
            try await Self.backend().reconnectPeer(deviceId: deviceId)
        }
    }

    // MARK: - Device nickname

    public func deviceSetNickname(deviceId: String, nickname: String) throws -> Promise<Void> {
        Promise.async {
            try await Self.backend().deviceSetNickname(deviceId: deviceId, nickname: nickname)
        }
    }

    // MARK: - Pair picker

    public func presentPairPicker() throws -> Promise<Variant_NullType_BridgethingBtDevice> {
        Promise.async {
            let device = try await Self.backend().presentPairPicker()
            return device.map { .second($0) } ?? .first(NullType.null)
        }
    }

    // MARK: - Notification access

    public func isNotificationAccessGranted() throws -> Promise<Bool> {
        Promise.async { await (try Self.backend()).isNotificationAccessGranted() }
    }

    public func requestNotificationAccess() throws -> Promise<Void> {
        Promise.async { try await Self.backend().requestNotificationAccess() }
    }

    public func isDefaultDialer() throws -> Promise<Bool> {
        Promise.async { await (try Self.backend()).isDefaultDialer() }
    }

    public func requestDefaultDialer() throws -> Promise<Void> {
        Promise.async { try await Self.backend().requestDefaultDialer() }
    }

    public func forgetCompanionDevice(mac: String) throws -> Promise<Void> {
        Promise.async { try await Self.backend().forgetCompanionDevice(mac: mac) }
    }

    public func isIgnoringBatteryOptimizations() throws -> Promise<Bool> {
        Promise.async { await (try Self.backend()).isIgnoringBatteryOptimizations() }
    }

    public func requestIgnoreBatteryOptimizations() throws -> Promise<Void> {
        Promise.async { try await Self.backend().requestIgnoreBatteryOptimizations() }
    }

    // MARK: - Runtime permission revoke

    public func revokeRuntimePermissions(permissions: [String]) throws -> Promise<Bool> {
        Promise.async { await (try Self.backend()).revokeRuntimePermissions(permissions: permissions) }
    }

    public func killApp() throws -> Promise<Void> {
        Promise.async { await (try Self.backend()).killApp() }
    }

    // MARK: - Callback setters

    public func setOnProviderChanged(callback: @escaping (Variant_NullType_BridgethingProviderInfo?) -> Void) throws {
        let wrapped: @Sendable (BridgethingProviderInfo?) -> Void = { info in
            callback(info.map { .second($0) } ?? .first(NullType.null))
        }
        Self.stateLock.lock()
        let backend = Self._backend
        if backend == nil { Self.pendingProviderChanged = wrapped }
        Self.stateLock.unlock()
        backend?.setOnProviderChanged(wrapped)
    }

    public func setOnAuthStateChanged(callback: @escaping (BridgethingAuthState) -> Void) throws {
        let wrapped: @Sendable (BridgethingAuthState) -> Void = { state in callback(state) }
        Self.stateLock.lock()
        let backend = Self._backend
        if backend == nil { Self.pendingAuthStateChanged = wrapped }
        Self.stateLock.unlock()
        backend?.setOnAuthStateChanged(wrapped)
    }

    public func setOnServiceHealthChanged(callback: @escaping (BridgethingServiceHealth) -> Void) throws {
        let wrapped: @Sendable (BridgethingServiceHealth) -> Void = { health in callback(health) }
        Self.stateLock.lock()
        let backend = Self._backend
        if backend == nil { Self.pendingServiceHealthChanged = wrapped }
        Self.stateLock.unlock()
        backend?.setOnServiceHealthChanged(wrapped)
    }

    public func setOnPeerConnected(callback: @escaping (BridgethingSessionPeer) -> Void) throws {
        let wrapped: @Sendable (BridgethingSessionPeer) -> Void = { peer in callback(peer) }
        Self.stateLock.lock()
        let backend = Self._backend
        if backend == nil { Self.pendingPeerConnected = wrapped }
        Self.stateLock.unlock()
        backend?.setOnPeerConnected(wrapped)
    }

    public func setOnPeerDisconnected(callback: @escaping (String) -> Void) throws {
        let wrapped: @Sendable (String) -> Void = { id in callback(id) }
        Self.stateLock.lock()
        let backend = Self._backend
        if backend == nil { Self.pendingPeerDisconnected = wrapped }
        Self.stateLock.unlock()
        backend?.setOnPeerDisconnected(wrapped)
    }

    public func setOnPeerLinkFailed(callback: @escaping (BridgethingSessionPeer) -> Void) throws {
        let wrapped: @Sendable (BridgethingSessionPeer) -> Void = { peer in callback(peer) }
        Self.stateLock.lock()
        let backend = Self._backend
        if backend == nil { Self.pendingPeerLinkFailed = wrapped }
        Self.stateLock.unlock()
        backend?.setOnPeerLinkFailed(wrapped)
    }

    public func setOnNowPlayingChanged(callback: @escaping (Variant_NullType_BridgethingNowPlaying?) -> Void) throws {
        let wrapped: @Sendable (BridgethingNowPlaying?) -> Void = { np in
            callback(np.map { .second($0) } ?? .first(NullType.null))
        }
        Self.stateLock.lock()
        let backend = Self._backend
        if backend == nil { Self.pendingNowPlayingChanged = wrapped }
        Self.stateLock.unlock()
        backend?.setOnNowPlayingChanged(wrapped)
    }

    public func setOnAncsAuthStatusChanged(callback: @escaping (BridgethingAncsAuthStatus) -> Void) throws {
        let wrapped: @Sendable (BridgethingAncsAuthStatus) -> Void = { status in callback(status) }
        Self.stateLock.lock()
        let backend = Self._backend
        if backend == nil { Self.pendingAncsAuthStatusChanged = wrapped }
        Self.stateLock.unlock()
        backend?.setOnAncsAuthStatusChanged(wrapped)
    }

    public func setOnLog(callback: @escaping (String, String) -> Void) throws {
        let wrapped: @Sendable (String, String) -> Void = { level, msg in callback(level, msg) }
        Self.stateLock.lock()
        let backend = Self._backend
        if backend == nil { Self.pendingLog = wrapped }
        Self.stateLock.unlock()
        backend?.setOnLog(wrapped)
    }

    public func setLogStreamingEnabled(enabled: Bool) throws {
        // pre-backend toggles are dropped; the backend installer sets initial state
        Self.stateLock.lock()
        let backend = Self._backend
        Self.stateLock.unlock()
        backend?.setLogStreamingEnabled(enabled)
    }

    public func setLocalLogStreamingEnabled(enabled: Bool) throws {
        // pre-backend toggles are dropped; the backend installer sets initial state
        Self.stateLock.lock()
        let backend = Self._backend
        Self.stateLock.unlock()
        backend?.setLocalLogStreamingEnabled(enabled)
    }

    public func setOnWebappsChanged(callback: @escaping (String) -> Void) throws {
        let wrapped: @Sendable (String) -> Void = { deviceId in callback(deviceId) }
        Self.stateLock.lock()
        let backend = Self._backend
        if backend == nil { Self.pendingWebappsChanged = wrapped }
        Self.stateLock.unlock()
        backend?.setOnWebappsChanged(wrapped)
    }

    public func setOnWebappDocChanged(callback: @escaping (String, String, String, Variant_NullType_String?) -> Void) throws {
        let wrapped: @Sendable (String, String, String, String?) -> Void = { deviceId, webappId, key, value in
            callback(deviceId, webappId, key, value.map { .second($0) } ?? .first(NullType.null))
        }
        Self.stateLock.lock()
        let backend = Self._backend
        if backend == nil { Self.pendingWebappDocChanged = wrapped }
        Self.stateLock.unlock()
        backend?.setOnWebappDocChanged(wrapped)
    }

    public func setOnDeviceMetaChanged(callback: @escaping (String, BridgethingDeviceMeta) -> Void) throws {
        let wrapped: @Sendable (String, BridgethingDeviceMeta) -> Void = { id, meta in callback(id, meta) }
        Self.stateLock.lock()
        let backend = Self._backend
        if backend == nil { Self.pendingDeviceMetaChanged = wrapped }
        Self.stateLock.unlock()
        backend?.setOnDeviceMetaChanged(wrapped)
    }

    public func setOnOtaEvent(callback: @escaping (BridgethingOtaEvent) -> Void) throws {
        let wrapped: @Sendable (BridgethingOtaEvent) -> Void = { event in callback(event) }
        Self.stateLock.lock()
        let backend = Self._backend
        if backend == nil { Self.pendingOtaEvent = wrapped }
        Self.stateLock.unlock()
        backend?.setOnOtaEvent(wrapped)
    }

    public func setOnCatalogEvent(callback: @escaping (BridgethingCatalogEvent) -> Void) throws {
        let wrapped: @Sendable (BridgethingCatalogEvent) -> Void = { event in callback(event) }
        Self.stateLock.lock()
        let backend = Self._backend
        if backend == nil { Self.pendingCatalogEvent = wrapped }
        Self.stateLock.unlock()
        backend?.setOnCatalogEvent(wrapped)
    }
}
