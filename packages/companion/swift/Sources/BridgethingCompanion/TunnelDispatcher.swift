import BridgethingGateway
import BridgethingSchema
import Foundation

#if canImport(Network)
    import Network
#endif

public actor TunnelDispatcher {
    private var openTask: Task<Void, Never>?
    private var dataTask: Task<Void, Never>?
    private var ackTask: Task<Void, Never>?
    private var closeTask: Task<Void, Never>?

    #if canImport(Network)
        private static let queue = DispatchQueue(label: "com.bridgething.tunnel", attributes: .concurrent)
        private static let ackIntervalBytes: UInt32 = 16 * 1024
        private static let ackStallSeconds: Double = 30
        private static let ackFlushNanos: UInt64 = 300_000_000
        private var connections: [UUID: NWConnection] = [:]
        private var pumps: [UUID: Task<Void, Never>] = [:]
        private var flushers: [UUID: Task<Void, Never>] = [:]
        private var delivered: [UUID: UInt32] = [:]
        private let acks = TransferAckWindow()
        private let connectTimeout: Duration

        public init(connectTimeout: Duration = .seconds(15)) {
            self.connectTimeout = connectTimeout
        }
    #else
        public init() {}
    #endif

    public func start(gateway: BridgethingGateway) async {
        openTask = Task { [weak self] in
            for await (handle, req) in gateway.tunnel.openRequests {
                Task { [weak self] in await self?.handleOpen(handle: handle, req: req, gateway: gateway) }
            }
        }
        dataTask = Task { [weak self] in
            for await (deviceId, msg) in gateway.tunnel.data {
                await self?.handleData(msg, deviceId: deviceId, gateway: gateway)
            }
        }
        ackTask = Task { [weak self] in
            for await (_, msg) in gateway.tunnel.ack {
                await self?.handleAck(msg)
            }
        }
        closeTask = Task { [weak self] in
            for await (_, msg) in gateway.tunnel.close {
                await self?.handleClose(msg)
            }
        }
    }

    public func stop() async {
        openTask?.cancel(); openTask = nil
        dataTask?.cancel(); dataTask = nil
        ackTask?.cancel(); ackTask = nil
        closeTask?.cancel(); closeTask = nil

        #if canImport(Network)
            for (_, pump) in pumps { pump.cancel() }
            pumps.removeAll()
            for (_, flusher) in flushers { flusher.cancel() }
            flushers.removeAll()
            delivered.removeAll()
            for (_, conn) in connections { conn.cancel() }
            connections.removeAll()
        #endif
    }

    // MARK: - inbound: Open (request)

    private func handleOpen(handle: TunnelOpenHandle, req: TunnelOpen, gateway: BridgethingGateway) async {
        #if canImport(Network)
            guard let port = NWEndpoint.Port(rawValue: req.port) else {
                try? await handle.respondErr(TunnelErrorReply(
                    error: .connectFailed(.init(reason: "invalid port \(req.port)"))))
                return
            }
            let conn = NWConnection(host: NWEndpoint.Host(req.host), port: port, using: .tcp)
            let id = req.tunnelId

            switch await connect(conn) {
            case .ready:
                connections[id] = conn
                try? await handle.respond(TunnelOpenReply())
                let pump = Task { [weak self] in
                    guard let self else { return }
                    await runPump(tunnelId: id, conn: conn, gateway: gateway)
                }
                pumps[id] = pump
                flushers[id] = Task { [weak self] in
                    while !Task.isCancelled {
                        try? await Task.sleep(nanoseconds: Self.ackFlushNanos)
                        guard !Task.isCancelled else { return }
                        await self?.flushAck(id, deviceId: handle.deviceId, gateway: gateway)
                    }
                }
            case let .failed(reason):
                conn.cancel()
                try? await handle.respondErr(TunnelErrorReply(error: .connectFailed(.init(reason: reason))))
            }
        #else
            try? await handle.respondErr(TunnelErrorReply(error: .unavailable))
        #endif
    }

    // MARK: - inbound: Data / Close (commands)

    private func handleData(_ msg: TunnelData, deviceId: String, gateway: BridgethingGateway) async {
        #if canImport(Network)
            guard let conn = connections[msg.tunnelId] else { return }
            let id = msg.tunnelId
            let count = UInt32(msg.bytes.count)
            conn.send(content: msg.bytes, completion: .contentProcessed { [weak self] _ in
                Task { await self?.noteDelivered(id, bytes: count, deviceId: deviceId, gateway: gateway) }
            })
        #endif
    }

    #if canImport(Network)
        private func noteDelivered(_ id: UUID, bytes: UInt32, deviceId: String, gateway: BridgethingGateway) async {
            guard connections[id] != nil else { return }
            delivered[id] = (delivered[id] ?? 0) + bytes
            guard delivered[id] ?? 0 >= Self.ackIntervalBytes else { return }
            await flushAck(id, deviceId: deviceId, gateway: gateway)
        }

        private func flushAck(_ id: UUID, deviceId: String, gateway: BridgethingGateway) async {
            guard let pending = delivered[id], pending > 0 else { return }
            delivered[id] = 0
            try? await gateway.device(deviceId).tunnel.ack(TunnelAck(tunnelId: id, consumed: pending))
        }

        private func handleAck(_ msg: TunnelAck) async {
            let total = await acks.receivedBytes(msg.tunnelId) + UInt64(msg.consumed)
            await acks.note(transferId: msg.tunnelId, received: total)
        }
    #else
        private func handleAck(_: TunnelAck) async {}
    #endif

    private func handleClose(_ msg: TunnelClosed) async {
        #if canImport(Network)
            teardown(msg.tunnelId)
        #endif
    }

    #if canImport(Network)
        // MARK: - byte pump (remote -> daemon)

        private func runPump(tunnelId: UUID, conn: NWConnection, gateway: BridgethingGateway) async {
            var pacer = TransferPacer()
            var sent: UInt64 = 0
            while !Task.isCancelled {
                pacer.observe(ackedBytes: await acks.receivedBytes(tunnelId))
                do {
                    try await acks.awaitWindow(
                        tunnelId,
                        offset: sent,
                        windowBytes: pacer.windowBytes,
                        timeoutSeconds: Self.ackStallSeconds
                    )
                } catch {
                    try? await gateway.tunnel.closed(
                        TunnelClosed(tunnelId: tunnelId, reason: "ack window stalled"), priority: .bulk)
                    teardown(tunnelId)
                    return
                }

                let outcome = await receive(conn, maxBytes: pacer.fragmentBytes)
                if Task.isCancelled { return }
                if let data = outcome.data, !data.isEmpty {
                    sent += UInt64(data.count)
                    try? await gateway.tunnel.data(
                        TunnelData(tunnelId: tunnelId, bytes: data), priority: .bulk)
                }
                if let reason = outcome.errorReason {
                    try? await gateway.tunnel.closed(
                        TunnelClosed(tunnelId: tunnelId, reason: reason), priority: .bulk)
                    teardown(tunnelId)
                    return
                }
                if outcome.isComplete {
                    try? await gateway.tunnel.closed(
                        TunnelClosed(tunnelId: tunnelId, reason: nil), priority: .bulk)
                    teardown(tunnelId)
                    return
                }
            }
        }

        private func teardown(_ id: UUID) {
            connections.removeValue(forKey: id)?.cancel()
            pumps.removeValue(forKey: id)?.cancel()
            flushers.removeValue(forKey: id)?.cancel()
            delivered.removeValue(forKey: id)
            Task { [acks] in await acks.finish(id) }
        }

        // MARK: - NWConnection async bridges

        private enum ConnectOutcome: Sendable {
            case ready
            case failed(String)
        }

        private struct ReceiveOutcome: Sendable {
            let data: Data?
            let isComplete: Bool
            let errorReason: String?
        }

        private func connect(_ conn: NWConnection) async -> ConnectOutcome {
            let box = OneShotBox<ConnectOutcome>()
            conn.stateUpdateHandler = { state in
                switch state {
                case .ready: box.fire(.ready)
                case let .failed(err): box.fire(.failed(err.localizedDescription))
                case let .waiting(err): box.fire(.failed(err.localizedDescription))
                case .cancelled: box.fire(.failed("cancelled"))
                default: break
                }
            }
            conn.start(queue: Self.queue)
            let timeout = Task { [connectTimeout] in
                try? await Task.sleep(for: connectTimeout)
                box.fire(.failed("connect timed out"))
            }
            let outcome = await withCheckedContinuation { (c: CheckedContinuation<ConnectOutcome, Never>) in
                box.register(c)
            }
            timeout.cancel()
            conn.stateUpdateHandler = nil
            return outcome
        }

        private func receive(_ conn: NWConnection, maxBytes: Int) async -> ReceiveOutcome {
            let box = OneShotBox<ReceiveOutcome>()
            conn.receive(minimumIncompleteLength: 1, maximumLength: maxBytes) { data, _, isComplete, error in
                box.fire(ReceiveOutcome(
                    data: data, isComplete: isComplete, errorReason: error?.localizedDescription))
            }
            return await withCheckedContinuation { (c: CheckedContinuation<ReceiveOutcome, Never>) in
                box.register(c)
            }
        }
    #endif
}

#if canImport(Network)
    private final class OneShotBox<T: Sendable>: @unchecked Sendable {
        private let lock = NSLock()
        private var cont: CheckedContinuation<T, Never>?
        private var stored: T?
        private var done = false

        func register(_ c: CheckedContinuation<T, Never>) {
            lock.lock()
            if done { lock.unlock(); return }
            if let value = stored {
                done = true
                stored = nil
                lock.unlock()
                c.resume(returning: value)
                return
            }
            cont = c
            lock.unlock()
        }

        func fire(_ value: T) {
            lock.lock()
            if done { lock.unlock(); return }
            if let c = cont {
                done = true
                cont = nil
                lock.unlock()
                c.resume(returning: value)
                return
            }
            stored = value
            lock.unlock()
        }
    }
#endif
