import BridgethingCompanion
import BridgethingGateway
import BridgethingGlue
import BridgethingLyrics
import Foundation
import NitroModules

/// iOS-side bridgething session module. Owns one `BridgethingCompanion`
/// (which owns the gateway, the active glue, and every dispatcher) and
/// exposes a Nitro-typed surface to the React Native UI shell.
///
/// Glue registration happens before Nitro starts: the host app's
/// `AppDelegate` populates `HybridBridgethingSession.registry` with one
/// `GlueFactory` per provider id, plus the `hostInfo` and `lyricsResolver`
/// singletons.
public final class HybridBridgethingSession: HybridBridgethingSessionSpec, @unchecked Sendable {
    public typealias GlueFactory = @Sendable () -> any BridgethingGlue

    public struct ProviderRegistration: Sendable {
        public let id: String
        public let displayName: String
        public let available: Bool
        public let factory: GlueFactory
        public init(id: String, displayName: String, available: Bool, factory: @escaping GlueFactory) {
            self.id = id
            self.displayName = displayName
            self.available = available
            self.factory = factory
        }
    }

    /// Populated by the host app's AppDelegate before any RN code runs.
    public static var registry: [ProviderRegistration] = []
    public static var hostInfo: HostInfo = .init(appName: "bridgething", appVersion: "0.0.0", osName: "iOS")
    public static var lyricsResolver: any LyricsResolver = LrclibResolver()
    public static var capabilityFlags: CompanionCapabilityFlags = .init()
    public static var eaProtocolString: String = "com.bridgething.gateway"

    private let stateLock = NSLock()
    private var companion: BridgethingCompanion?
    private var eventsTask: Task<Void, Never>?
    private var peers: [String: BridgethingSessionPeer] = [:]

    private var onProviderChanged: ((BridgethingProviderInfo?) -> Void)?
    private var onAuthStateChanged: ((BridgethingAuthState) -> Void)?
    private var onPeerConnected: ((BridgethingSessionPeer) -> Void)?
    private var onPeerDisconnected: ((String) -> Void)?
    private var onLog: ((String, String) -> Void)?

    override public init() { super.init() }

    // MARK: - Lifecycle

    public func start() throws -> Promise<Void> {
        Promise.async { [self] in
            let adapter = EAAccessoryAdapter(protocolString: Self.eaProtocolString)
            let companion = BridgethingCompanion(
                adapter: adapter,
                lyricsResolver: Self.lyricsResolver,
                host: Self.hostInfo,
                capabilities: Self.capabilityFlags
            )
            stateLock.lock()
            self.companion = companion
            stateLock.unlock()

            try await companion.start()

            let task = Task { [weak self] in
                for await event in companion.gateway.events {
                    self?.handleGatewayEvent(event)
                }
            }
            stateLock.lock()
            eventsTask = task
            stateLock.unlock()
        }
    }

    public func stop() throws -> Promise<Void> {
        Promise.async { [self] in
            let task = stateLock.withLock { eventsTask }
            task?.cancel()

            let companion = stateLock.withLock { () -> BridgethingCompanion? in
                let c = self.companion
                self.companion = nil
                self.eventsTask = nil
                return c
            }
            await companion?.stop()
            stateLock.withLock { peers.removeAll() }
        }
    }

    // MARK: - Provider selection

    public func availableProviders() throws -> Promise<[BridgethingProviderInfo]> {
        Promise.resolved(withResult: Self.registry.map {
            BridgethingProviderInfo(id: $0.id, displayName: $0.displayName, available: $0.available)
        })
    }

    public func setActiveProvider(id: String?) throws -> Promise<Void> {
        Promise.async { [self] in
            let companion = stateLock.withLock { self.companion }
            guard let companion else { return }

            if let id {
                guard let registration = Self.registry.first(where: { $0.id == id }) else {
                    throw RuntimeError.error(withMessage: "unknown provider \(id)")
                }
                emitAuth(BridgethingAuthState(kind: .pending, userCode: nil, verificationUrl: nil, message: nil))
                do {
                    let glue = registration.factory()
                    try await companion.setActive(glue)
                    emitProvider(BridgethingProviderInfo(
                        id: registration.id,
                        displayName: registration.displayName,
                        available: registration.available
                    ))
                    emitAuth(BridgethingAuthState(kind: .authenticated, userCode: nil, verificationUrl: nil, message: nil))
                } catch {
                    emitAuth(BridgethingAuthState(kind: .failed, userCode: nil, verificationUrl: nil, message: String(describing: error)))
                    throw RuntimeError.error(withMessage: String(describing: error))
                }
            } else {
                try await companion.setActive(nil)
                emitProvider(nil)
                emitAuth(BridgethingAuthState(kind: .idle, userCode: nil, verificationUrl: nil, message: nil))
            }
        }
    }

    public func currentProvider() throws -> Promise<BridgethingProviderInfo?> {
        Promise.async { [self] in
            let companion = stateLock.withLock { self.companion }
            let glue = await companion?.current()
            return providerInfo(for: glue)
        }
    }

    public func connectedPeers() throws -> Promise<[BridgethingSessionPeer]> {
        Promise.resolved(withResult: stateLock.withLock { Array(peers.values) })
    }

    // MARK: - Callback setters

    public func setOnProviderChanged(callback: @escaping (BridgethingProviderInfo?) -> Void) throws {
        stateLock.withLock { onProviderChanged = callback }
    }

    public func setOnAuthStateChanged(callback: @escaping (BridgethingAuthState) -> Void) throws {
        stateLock.withLock { onAuthStateChanged = callback }
    }

    public func setOnPeerConnected(callback: @escaping (BridgethingSessionPeer) -> Void) throws {
        stateLock.withLock { onPeerConnected = callback }
    }

    public func setOnPeerDisconnected(callback: @escaping (String) -> Void) throws {
        stateLock.withLock { onPeerDisconnected = callback }
    }

    public func setOnLog(callback: @escaping (String, String) -> Void) throws {
        stateLock.withLock { onLog = callback }
    }

    // MARK: - Internal

    private func handleGatewayEvent(_ event: GatewayEvent) {
        switch event {
        case let .connected(device):
            let peer = BridgethingSessionPeer(id: device.id, name: device.name)
            stateLock.withLock { peers[device.id] = peer }
            stateLock.withLock { onPeerConnected }?(peer)
        case let .disconnected(id):
            stateLock.withLock { _ = peers.removeValue(forKey: id) }
            stateLock.withLock { onPeerDisconnected }?(id)
        case .message:
            break
        case let .decodeError(id, description):
            stateLock.withLock { onLog }?("warn", "[\(id)] decode error: \(description)")
        }
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

    private func emitProvider(_ info: BridgethingProviderInfo?) {
        stateLock.withLock { onProviderChanged }?(info)
    }

    private func emitAuth(_ state: BridgethingAuthState) {
        stateLock.withLock { onAuthStateChanged }?(state)
    }
}

private extension NSLock {
    @discardableResult
    func withLock<T>(_ body: () throws -> T) rethrows -> T {
        lock(); defer { unlock() }
        return try body()
    }
}
