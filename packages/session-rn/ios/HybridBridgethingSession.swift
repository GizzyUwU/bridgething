import Foundation
import NitroModules

/// Backend protocol the host app implements. Decouples the Nitro
/// HybridObject (lives in this pod, can't see SwiftPM packages) from
/// the orchestration logic (lives in the host app target, where the
/// bridgething Swift packages are linked via SPM).
///
/// The host app implements `BridgethingSessionBackend` and registers
/// it with `HybridBridgethingSession.installBackend(_:)` at app launch
/// (before any JS code runs).
public protocol BridgethingSessionBackend: AnyObject, Sendable {
    func start() async throws
    func stop() async

    func availableProviders() async -> [BridgethingProviderInfo]
    func setActiveProvider(id: String?) async throws
    func cancelAuth() async
    func signOut() async
    func currentProvider() async -> BridgethingProviderInfo?
    func connectedPeers() async -> [BridgethingSessionPeer]
    func currentNowPlaying() async -> BridgethingNowPlaying?

    func enableAncsNotifications() async -> BridgethingAncsSetupResult
    func ancsAuthStatus() async -> BridgethingAncsAuthStatus

    // Webapps (per-device)
    func listWebapps(deviceId: String) async throws -> [BridgethingWebappInfo]
    func currentWebapp(deviceId: String) async throws -> BridgethingActiveWebapp?
    func installWebappFromBase64(deviceId: String, archiveBase64: String) async throws -> BridgethingWebappInfo
    func uninstallWebapp(deviceId: String, id: String) async throws
    func switchWebapp(deviceId: String, id: String) async throws
    func webappIcon(deviceId: String, id: String) async throws -> BridgethingWebappIcon?
    func listWebappConfig(deviceId: String, id: String) async throws -> [BridgethingConfigEntry]
    func setWebappConfigField(deviceId: String, id: String, key: String, value: String) async throws
    func deleteWebappConfigField(deviceId: String, id: String, key: String) async throws

    // Capability flags
    func setCapabilityFlags(flags: BridgethingCapabilityFlags) async

    // OTA
    func setOtaPollConfig(config: BridgethingOtaPollConfig?) async
    func pollOtaNow() async
    func deviceMeta(deviceId: String) async -> BridgethingDeviceMeta?

    // Host identity
    func hostInfo() async -> BridgethingHostInfo

    // In-app Bluetooth pairing (android-only in spec; iOS impls reject as unsupported)
    func listBondedBluetoothDevices() async -> [BridgethingBtDevice]
    func startBluetoothDiscovery() async throws
    func stopBluetoothDiscovery() async
    func pairBluetoothDevice(address: String) async throws -> BridgethingBtBondState
    func presentPairPicker() async throws -> BridgethingBtDevice?

    // Notification access (android-only in spec; iOS impls reject as unsupported)
    func isNotificationAccessGranted() async -> Bool
    func requestNotificationAccess() async throws

    // Runtime perm revoke (android-only; iOS returns false / no-op)
    func revokeRuntimePermissions(permissions: [String]) async -> Bool
    func killApp() async

    func setOnProviderChanged(_ callback: @escaping @Sendable (BridgethingProviderInfo?) -> Void)
    func setOnAuthStateChanged(_ callback: @escaping @Sendable (BridgethingAuthState) -> Void)
    func setOnPeerConnected(_ callback: @escaping @Sendable (BridgethingSessionPeer) -> Void)
    func setOnPeerDisconnected(_ callback: @escaping @Sendable (String) -> Void)
    func setOnNowPlayingChanged(_ callback: @escaping @Sendable (BridgethingNowPlaying?) -> Void)
    func setOnAncsAuthStatusChanged(_ callback: @escaping @Sendable (BridgethingAncsAuthStatus) -> Void)
    func setOnLog(_ callback: @escaping @Sendable (String, String) -> Void)
    func setLogStreamingEnabled(_ enabled: Bool)

    func setOnWebappsChanged(_ callback: @escaping @Sendable (String) -> Void)
    func setOnDeviceMetaChanged(_ callback: @escaping @Sendable (String, BridgethingDeviceMeta) -> Void)
    func setOnOtaEvent(_ callback: @escaping @Sendable (BridgethingOtaEvent) -> Void)
    func setOnBluetoothDiscoveryEvent(_ callback: @escaping @Sendable (BridgethingBtDiscoveryEvent) -> Void)
    func setOnBluetoothBondStateChanged(_ callback: @escaping @Sendable (BridgethingBtDevice) -> Void)
}

/// Thin Nitro proxy. The pod ships this; the host app installs a
/// `BridgethingSessionBackend` at launch. Without a backend installed,
/// every method throws "backend not installed". Callback setters are
/// silently buffered until a backend is installed, then re-applied.
public final class HybridBridgethingSession: HybridBridgethingSessionSpec, @unchecked Sendable {
    private static let stateLock = NSLock()
    private static var _backend: (any BridgethingSessionBackend)?

    private static var pendingProviderChanged: (@Sendable (BridgethingProviderInfo?) -> Void)?
    private static var pendingAuthStateChanged: (@Sendable (BridgethingAuthState) -> Void)?
    private static var pendingPeerConnected: (@Sendable (BridgethingSessionPeer) -> Void)?
    private static var pendingPeerDisconnected: (@Sendable (String) -> Void)?
    private static var pendingNowPlayingChanged: (@Sendable (BridgethingNowPlaying?) -> Void)?
    private static var pendingAncsAuthStatusChanged: (@Sendable (BridgethingAncsAuthStatus) -> Void)?
    private static var pendingLog: (@Sendable (String, String) -> Void)?
    private static var pendingWebappsChanged: (@Sendable (String) -> Void)?
    private static var pendingDeviceMetaChanged: (@Sendable (String, BridgethingDeviceMeta) -> Void)?
    private static var pendingOtaEvent: (@Sendable (BridgethingOtaEvent) -> Void)?
    private static var pendingBtDiscoveryEvent: (@Sendable (BridgethingBtDiscoveryEvent) -> Void)?
    private static var pendingBtBondStateChanged: (@Sendable (BridgethingBtDevice) -> Void)?

    /// Host apps call this once at launch (before React Native starts)
    /// to wire up the real session backend that uses the bridgething
    /// Swift packages. Replays any callback setters JS may have already
    /// installed.
    public static func installBackend(_ backend: any BridgethingSessionBackend) {
        stateLock.lock()
        _backend = backend
        let providerCb = pendingProviderChanged
        let authCb = pendingAuthStateChanged
        let peerConnCb = pendingPeerConnected
        let peerDisconnCb = pendingPeerDisconnected
        let nowPlayingCb = pendingNowPlayingChanged
        let ancsCb = pendingAncsAuthStatusChanged
        let logCb = pendingLog
        let webappsCb = pendingWebappsChanged
        let deviceMetaCb = pendingDeviceMetaChanged
        let otaCb = pendingOtaEvent
        let btDiscCb = pendingBtDiscoveryEvent
        let btBondCb = pendingBtBondStateChanged
        pendingProviderChanged = nil
        pendingAuthStateChanged = nil
        pendingPeerConnected = nil
        pendingPeerDisconnected = nil
        pendingNowPlayingChanged = nil
        pendingAncsAuthStatusChanged = nil
        pendingLog = nil
        pendingWebappsChanged = nil
        pendingDeviceMetaChanged = nil
        pendingOtaEvent = nil
        pendingBtDiscoveryEvent = nil
        pendingBtBondStateChanged = nil
        stateLock.unlock()

        if let providerCb { backend.setOnProviderChanged(providerCb) }
        if let authCb { backend.setOnAuthStateChanged(authCb) }
        if let peerConnCb { backend.setOnPeerConnected(peerConnCb) }
        if let peerDisconnCb { backend.setOnPeerDisconnected(peerDisconnCb) }
        if let nowPlayingCb { backend.setOnNowPlayingChanged(nowPlayingCb) }
        if let ancsCb { backend.setOnAncsAuthStatusChanged(ancsCb) }
        if let logCb { backend.setOnLog(logCb) }
        if let webappsCb { backend.setOnWebappsChanged(webappsCb) }
        if let deviceMetaCb { backend.setOnDeviceMetaChanged(deviceMetaCb) }
        if let otaCb { backend.setOnOtaEvent(otaCb) }
        if let btDiscCb { backend.setOnBluetoothDiscoveryEvent(btDiscCb) }
        if let btBondCb { backend.setOnBluetoothBondStateChanged(btBondCb) }
    }

    private static func backend() throws -> any BridgethingSessionBackend {
        stateLock.lock(); defer { stateLock.unlock() }
        guard let b = _backend else {
            throw RuntimeError.error(withMessage: "BridgethingSession backend not installed - host app must call HybridBridgethingSession.installBackend(_:) before React Native starts")
        }
        return b
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

    public func connectedPeers() throws -> Promise<[BridgethingSessionPeer]> {
        Promise.async {
            await (try Self.backend()).connectedPeers()
        }
    }

    public func currentNowPlaying() throws -> Promise<Variant_NullType_BridgethingNowPlaying> {
        Promise.async {
            let np = await (try Self.backend()).currentNowPlaying()
            return np.map { .second($0) } ?? .first(NullType.null)
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

    public func installWebappFromBase64(deviceId: String, archiveBase64: String) throws -> Promise<BridgethingWebappInfo> {
        Promise.async {
            try await Self.backend().installWebappFromBase64(deviceId: deviceId, archiveBase64: archiveBase64)
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

    // MARK: - Capability flags

    public func setCapabilityFlags(flags: BridgethingCapabilityFlags) throws -> Promise<Void> {
        Promise.async {
            await (try Self.backend()).setCapabilityFlags(flags: flags)
        }
    }

    // MARK: - OTA

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

    public func pollOtaNow() throws -> Promise<Void> {
        Promise.async {
            await (try Self.backend()).pollOtaNow()
        }
    }

    public func deviceMeta(deviceId: String) throws -> Promise<Variant_NullType_BridgethingDeviceMeta> {
        Promise.async {
            let meta = await (try Self.backend()).deviceMeta(deviceId: deviceId)
            return meta.map { .second($0) } ?? .first(NullType.null)
        }
    }

    public func hostInfo() throws -> Promise<BridgethingHostInfo> {
        Promise.async {
            await (try Self.backend()).hostInfo()
        }
    }

    // MARK: - In-app Bluetooth pairing (android-only in spec)

    public func listBondedBluetoothDevices() throws -> Promise<[BridgethingBtDevice]> {
        Promise.async { await (try Self.backend()).listBondedBluetoothDevices() }
    }

    public func startBluetoothDiscovery() throws -> Promise<Void> {
        Promise.async { try await Self.backend().startBluetoothDiscovery() }
    }

    public func stopBluetoothDiscovery() throws -> Promise<Void> {
        Promise.async { await (try Self.backend()).stopBluetoothDiscovery() }
    }

    public func pairBluetoothDevice(address: String) throws -> Promise<BridgethingBtBondState> {
        Promise.async { try await Self.backend().pairBluetoothDevice(address: address) }
    }

    public func presentPairPicker() throws -> Promise<BridgethingBtDevice?> {
        Promise.async { try await Self.backend().presentPairPicker() }
    }

    // MARK: - Notification access (android-only in spec)

    public func isNotificationAccessGranted() throws -> Promise<Bool> {
        Promise.async { await (try Self.backend()).isNotificationAccessGranted() }
    }

    public func requestNotificationAccess() throws -> Promise<Void> {
        Promise.async { try await Self.backend().requestNotificationAccess() }
    }

    // MARK: - Runtime perm revoke (android-only)

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
        // Pre-backend toggles are dropped; whoever installs the backend
        // is responsible for the initial stream state.
        Self.stateLock.lock()
        let backend = Self._backend
        Self.stateLock.unlock()
        backend?.setLogStreamingEnabled(enabled)
    }

    public func setOnWebappsChanged(callback: @escaping (String) -> Void) throws {
        let wrapped: @Sendable (String) -> Void = { deviceId in callback(deviceId) }
        Self.stateLock.lock()
        let backend = Self._backend
        if backend == nil { Self.pendingWebappsChanged = wrapped }
        Self.stateLock.unlock()
        backend?.setOnWebappsChanged(wrapped)
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

    public func setOnBluetoothDiscoveryEvent(callback: @escaping (BridgethingBtDiscoveryEvent) -> Void) throws {
        let wrapped: @Sendable (BridgethingBtDiscoveryEvent) -> Void = { event in callback(event) }
        Self.stateLock.lock()
        let backend = Self._backend
        if backend == nil { Self.pendingBtDiscoveryEvent = wrapped }
        Self.stateLock.unlock()
        backend?.setOnBluetoothDiscoveryEvent(wrapped)
    }

    public func setOnBluetoothBondStateChanged(callback: @escaping (BridgethingBtDevice) -> Void) throws {
        let wrapped: @Sendable (BridgethingBtDevice) -> Void = { device in callback(device) }
        Self.stateLock.lock()
        let backend = Self._backend
        if backend == nil { Self.pendingBtBondStateChanged = wrapped }
        Self.stateLock.unlock()
        backend?.setOnBluetoothBondStateChanged(wrapped)
    }
}
