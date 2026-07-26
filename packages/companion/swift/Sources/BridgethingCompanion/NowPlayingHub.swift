import BridgethingGateway
import BridgethingGlue
import BridgethingSchema
import Foundation

public final class NowPlayingHub: NowPlayingSink, @unchecked Sendable {
    private final class SourceState {
        var snapshot: PlayerState?
        var appBundle: String = ""
        var hasItem: Bool = false
        var wantsVolume: Bool = false
        var queue: QueueSnapshot?
        var targets: PlaybackTargets?
        var seq: Int64 = 0
    }

    private enum Op: Sendable {
        case player(sourceId: String, snapshot: PlayerState, appBundle: String, hasItem: Bool, wantsVolume: Bool)
        case queue(sourceId: String, queue: QueueSnapshot)
        case targets(sourceId: String, targets: PlaybackTargets)
        case clear(sourceId: String)
        case reconnect
    }

    private let gateway: BridgethingGateway
    private let ops: AsyncStream<Op>
    private let submit: AsyncStream<Op>.Continuation

    private var sources: [String: SourceState] = [:]
    private var seqCounter: Int64 = 0
    private var heldScopes: Set<CompanionAuthorityScope> = []
    private var claimedBundle: String?

    private let lock = NSLock()
    private var consumer: Task<Void, Never>?
    private var currentId: String?
    private var transports: [String: any NowPlayingTransport] = [:]

    public init(gateway: BridgethingGateway) {
        self.gateway = gateway
        var cont: AsyncStream<Op>.Continuation!
        ops = AsyncStream(bufferingPolicy: .unbounded) { cont = $0 }
        submit = cont
    }

    public func start() {
        lock.lock()
        guard consumer == nil else { lock.unlock(); return }
        let stream = ops
        let task = Task { [weak self] in
            for await op in stream {
                guard let self else { return }
                await handle(op)
            }
        }
        consumer = task
        lock.unlock()
    }

    public func stop() {
        lock.lock()
        let task = consumer
        consumer = nil
        lock.unlock()
        task?.cancel()
    }

    public func submitPlayer(
        sourceId: String,
        snapshot: PlayerState,
        appBundle: String,
        hasItem: Bool,
        wantsVolume: Bool
    ) {
        submit.yield(.player(
            sourceId: sourceId,
            snapshot: snapshot,
            appBundle: appBundle,
            hasItem: hasItem,
            wantsVolume: wantsVolume
        ))
    }

    public func submitQueue(sourceId: String, queue: QueueSnapshot) {
        submit.yield(.queue(sourceId: sourceId, queue: queue))
    }

    public func submitTargets(sourceId: String, targets: PlaybackTargets) {
        submit.yield(.targets(sourceId: sourceId, targets: targets))
    }

    public func clearSource(sourceId: String) {
        submit.yield(.clear(sourceId: sourceId))
    }

    public func onConnect() {
        submit.yield(.reconnect)
    }

    public func register(sourceId: String, transport: any NowPlayingTransport) {
        lock.lock()
        transports[sourceId] = transport
        lock.unlock()
    }

    public func unregister(sourceId: String) {
        lock.lock()
        transports.removeValue(forKey: sourceId)
        lock.unlock()
    }

    public func currentTransport() -> (any NowPlayingTransport)? {
        lock.lock()
        defer { lock.unlock() }
        guard let currentId else { return nil }
        return transports[currentId]
    }

    public func currentSource() -> String? {
        lock.lock()
        defer { lock.unlock() }
        return currentId
    }

    private func setCurrent(_ id: String?) {
        lock.lock()
        currentId = id
        lock.unlock()
    }

    private func handle(_ op: Op) async {
        switch op {
        case let .player(sourceId, snapshot, appBundle, hasItem, wantsVolume):
            let s = sources[sourceId] ?? SourceState()
            s.snapshot = snapshot
            s.appBundle = appBundle
            s.hasItem = hasItem
            s.wantsVolume = wantsVolume
            seqCounter += 1
            s.seq = seqCounter
            sources[sourceId] = s
            await emitArbitrated()
        case let .queue(sourceId, queue):
            let s = sources[sourceId] ?? SourceState()
            s.queue = queue
            sources[sourceId] = s
            let cur = currentSource()
            if cur == nil || cur == sourceId {
                try? await gateway.player.queueChanged(queue)
            }
        case let .targets(sourceId, targets):
            let s = sources[sourceId] ?? SourceState()
            s.targets = targets
            sources[sourceId] = s
            let cur = currentSource()
            if cur == nil || cur == sourceId {
                try? await gateway.player.targetsChanged(targets)
            }
        case let .clear(sourceId):
            sources.removeValue(forKey: sourceId)
            if sourceId == currentSource() {
                setCurrent(nil)
                await emitArbitrated()
            }
        case .reconnect:
            heldScopes.removeAll()
            claimedBundle = nil
            await reemitCurrent()
        }
    }

    private func emitArbitrated() async {
        let prev = currentSource()
        let next = pickCurrent()
        setCurrent(next)
        guard let next, let s = sources[next] else {
            await releaseAll()
            return
        }
        if s.hasItem { await claim(s.appBundle, wantsVolume: s.wantsVolume) } else { await releaseAll() }
        if let snapshot = s.snapshot {
            try? await gateway.player.snapshot(snapshot)
        }
        if let prev, next != prev {
            try? await gateway.player.queueChanged(s.queue ?? QueueSnapshot(order: [], items: []))
            try? await gateway.player.targetsChanged(s.targets ?? PlaybackTargets(targets: []))
        }
    }

    private func reemitCurrent() async {
        var next = currentSource()
        if next == nil {
            next = pickCurrent()
            setCurrent(next)
        }
        guard let next, let s = sources[next] else { return }
        if s.hasItem { await claim(s.appBundle, wantsVolume: s.wantsVolume) }
        if let snapshot = s.snapshot { try? await gateway.player.snapshot(snapshot) }
        if let queue = s.queue { try? await gateway.player.queueChanged(queue) }
        if let targets = s.targets { try? await gateway.player.targetsChanged(targets) }
    }

    private func pickCurrent() -> String? {
        if sources.isEmpty { return nil }
        let playing = sources.filter { $0.value.hasItem && $0.value.snapshot?.playback.state == .playing }
        let pool = playing.isEmpty ? sources : playing
        return pool.max { a, b in a.value.seq < b.value.seq }?.key
    }

    private func claim(_ appBundle: String, wantsVolume: Bool) async {
        let bundleChanged = claimedBundle != appBundle
        var want = Self.nowPlayingScopes
        if wantsVolume { want.append(.volume) }
        for scope in want where !heldScopes.contains(scope) || bundleChanged {
            do {
                try await gateway.authority.claim(AuthorityClaim(scope: scope, appBundle: appBundle))
                heldScopes.insert(scope)
            } catch {
                continue
            }
        }
        if !wantsVolume, heldScopes.remove(.volume) != nil {
            try? await gateway.authority.release(AuthorityRelease(scope: .volume))
        }
        claimedBundle = appBundle
    }

    private func releaseAll() async {
        claimedBundle = nil
        guard !heldScopes.isEmpty else { return }
        let scopes = heldScopes
        heldScopes.removeAll()
        for scope in scopes {
            try? await gateway.authority.release(AuthorityRelease(scope: scope))
        }
    }

    private static let nowPlayingScopes: [CompanionAuthorityScope] = [
        .nowPlayingPlayback,
        .nowPlayingMetadata,
    ]
}
