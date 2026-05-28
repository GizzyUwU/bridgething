import BridgethingGateway
import BridgethingSchema
import Foundation

public enum WireDriverError: Error, Sendable {
    case timeout
    case decodeFailed
}

public actor WireDriver {
    private let adapter: InMemoryAdapter
    private let codec: Codec
    public let deviceId: String

    private var pending: [UUID: CheckedContinuation<GatewayToBridgeMsg, Error>] = [:]
    private var outbound: [GatewayToBridgeMsg] = []
    private var waiters: [(id: UUID, predicate: @Sendable (GatewayToBridgeMsg) -> Bool, cont: CheckedContinuation<GatewayToBridgeMsg, Error>)] = []
    private var pumpTask: Task<Void, Never>?

    public init(
        adapter: InMemoryAdapter,
        deviceId: String = "carthing-test",
        codec: Codec = Codec()
    ) {
        self.adapter = adapter
        self.deviceId = deviceId
        self.codec = codec
    }

    /// Begin draining outbound frames. Call once, after the companion is started.
    public func start() {
        guard pumpTask == nil else { return }
        let stream = adapter.sentFrames
        let codec = self.codec
        pumpTask = Task { [weak self] in
            for await (_, frame) in stream {
                guard let msg = try? codec.decode(GatewayToBridgeMsg.self, from: frame) else { continue }
                await self?.route(msg)
            }
        }
    }

    /// Simulate the daemon connecting over the transport.
    public nonisolated func connect(name: String = "Car Thing") {
        adapter.connect(Device(id: deviceId, name: name))
    }

    private func route(_ msg: GatewayToBridgeMsg) {
        if case let .response(meta) = msg.meta, let cont = pending.removeValue(forKey: meta.requestId) {
            cont.resume(returning: msg)
            return
        }
        if let idx = waiters.firstIndex(where: { $0.predicate(msg) }) {
            let waiter = waiters.remove(at: idx)
            waiter.cont.resume(returning: msg)
            return
        }
        outbound.append(msg)
    }

    private func failPending(_ id: UUID, _ error: Error) {
        if let cont = pending.removeValue(forKey: id) { cont.resume(throwing: error) }
    }

    private func failWaiter(_ id: UUID, _ error: Error) {
        if let idx = waiters.firstIndex(where: { $0.id == id }) {
            let waiter = waiters.remove(at: idx)
            waiter.cont.resume(throwing: error)
        }
    }

    /// Send a `.request` and await the matching `.response` frame, or throw on timeout.
    @discardableResult
    public func request(_ data: BridgeToGatewayMsgData, timeout: Duration = .seconds(5)) async throws -> GatewayToBridgeMsg {
        let id = UUID()
        let msg = BridgeToGatewayMsg(id: id, meta: .request, data: data)
        let frame = try codec.encode(msg)
        return try await withCheckedThrowingContinuation { cont in
            pending[id] = cont
            adapter.feed(deviceId: deviceId, frame)
            Task { [weak self] in
                try? await Task.sleep(for: timeout)
                await self?.failPending(id, WireDriverError.timeout)
            }
        }
    }

    /// Send a fire-and-forget command/event frame (no response expected).
    public func send(_ data: BridgeToGatewayMsgData, meta: MsgMeta = .command) async throws {
        let msg = BridgeToGatewayMsg(id: UUID(), meta: meta, data: data)
        adapter.feed(deviceId: deviceId, try codec.encode(msg))
    }

    /// Await the next outbound frame matching `predicate` (drains any already buffered first).
    public func waitOutbound(
        timeout: Duration = .seconds(5),
        where predicate: @escaping @Sendable (GatewayToBridgeMsg) -> Bool
    ) async throws -> GatewayToBridgeMsg {
        if let idx = outbound.firstIndex(where: predicate) {
            return outbound.remove(at: idx)
        }
        let id = UUID()
        return try await withCheckedThrowingContinuation { cont in
            waiters.append((id: id, predicate: predicate, cont: cont))
            Task { [weak self] in
                try? await Task.sleep(for: timeout)
                await self?.failWaiter(id, WireDriverError.timeout)
            }
        }
    }

    /// Snapshot of outbound frames seen so far (responses are consumed by `request`).
    public func outboundFrames() -> [GatewayToBridgeMsg] { outbound }

    public func stop() {
        pumpTask?.cancel()
        pumpTask = nil
    }
}
