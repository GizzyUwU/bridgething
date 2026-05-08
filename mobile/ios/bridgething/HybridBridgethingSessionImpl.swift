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
    public static var capabilityFlags: CompanionCapabilityFlags = .init()
    public static var eaProtocolString: String = "com.bridgething.gateway"

    private let stateLock = NSLock()
    private var companion: BridgethingCompanion?
    private var eventsTask: Task<Void, Never>?
    private var authTask: Task<Void, Never>?
    private var peers: [String: BridgethingSessionPeer] = [:]
    private var lastNowPlaying: BridgethingNowPlaying?

    private var onProviderChanged: (@Sendable (BridgethingProviderInfo?) -> Void)?
    private var onAuthStateChanged: (@Sendable (BridgethingAuthState) -> Void)?
    private var onPeerConnected: (@Sendable (BridgethingSessionPeer) -> Void)?
    private var onPeerDisconnected: (@Sendable (String) -> Void)?
    private var onNowPlayingChanged: (@Sendable (BridgethingNowPlaying?) -> Void)?
    private var onAncsAuthStatusChanged: (@Sendable (BridgethingAncsAuthStatus) -> Void)?
    private var onLog: (@Sendable (String, String) -> Void)?

    public init() {}

    // MARK: - Lifecycle

    public func start() async throws {
        let adapter = EAAccessoryAdapter(protocolString: Self.eaProtocolString)
        let companion = BridgethingCompanion(
            adapter: adapter,
            lyricsResolver: Self.lyricsResolver,
            host: Self.hostInfo,
            capabilities: Self.capabilityFlags
        )
        stateLock.lock(); self.companion = companion; stateLock.unlock()

        await companion.setNowPlayingObserver { [weak self] np in
            self?.handleNowPlaying(np)
        }
        await companion.setAncsAuthStateObserver { [weak self] state in
            self?.emitAncsAuthStatus(toRNAncsAuthStatus(state))
        }

        try await companion.start()

        let events = companion.gateway.events
        let task = Task { [weak self] in
            for await event in events {
                self?.handleGatewayEvent(event)
            }
        }
        stateLock.lock(); eventsTask = task; stateLock.unlock()
    }

    public func stop() async {
        stateLock.lock()
        let auth = authTask
        let events = eventsTask
        let companion = self.companion
        self.companion = nil
        eventsTask = nil
        authTask = nil
        stateLock.unlock()

        auth?.cancel()
        events?.cancel()

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
            let peer = BridgethingSessionPeer(id: device.id, name: device.name)
            stateLock.withLock { peers[device.id] = peer }
            emitPeerConnected(peer)
        case let .disconnected(id):
            stateLock.withLock { _ = peers.removeValue(forKey: id) }
            emitPeerDisconnected(id)
        case .message:
            break
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
}

private enum SessionError: Error {
    case deallocated
    case cancelled
    case notStarted
    case unknownProvider(String)
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
