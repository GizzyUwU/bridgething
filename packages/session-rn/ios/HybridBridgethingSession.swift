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

    func setOnProviderChanged(_ callback: @escaping @Sendable (BridgethingProviderInfo?) -> Void)
    func setOnAuthStateChanged(_ callback: @escaping @Sendable (BridgethingAuthState) -> Void)
    func setOnPeerConnected(_ callback: @escaping @Sendable (BridgethingSessionPeer) -> Void)
    func setOnPeerDisconnected(_ callback: @escaping @Sendable (String) -> Void)
    func setOnNowPlayingChanged(_ callback: @escaping @Sendable (BridgethingNowPlaying?) -> Void)
    func setOnAncsAuthStatusChanged(_ callback: @escaping @Sendable (BridgethingAncsAuthStatus) -> Void)
    func setOnLog(_ callback: @escaping @Sendable (String, String) -> Void)
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
        pendingProviderChanged = nil
        pendingAuthStateChanged = nil
        pendingPeerConnected = nil
        pendingPeerDisconnected = nil
        pendingNowPlayingChanged = nil
        pendingAncsAuthStatusChanged = nil
        pendingLog = nil
        stateLock.unlock()

        if let providerCb { backend.setOnProviderChanged(providerCb) }
        if let authCb { backend.setOnAuthStateChanged(authCb) }
        if let peerConnCb { backend.setOnPeerConnected(peerConnCb) }
        if let peerDisconnCb { backend.setOnPeerDisconnected(peerDisconnCb) }
        if let nowPlayingCb { backend.setOnNowPlayingChanged(nowPlayingCb) }
        if let ancsCb { backend.setOnAncsAuthStatusChanged(ancsCb) }
        if let logCb { backend.setOnLog(logCb) }
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
}
