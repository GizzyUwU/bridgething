import BridgethingGateway
import BridgethingSchema
import Foundation

#if canImport(Network)
    import Network
#endif

public actor TunnelDispatcher {
    private var openTask: Task<Void, Never>?
    private var dataTask: Task<Void, Never>?
    private var closeTask: Task<Void, Never>?

    #if canImport(Network)
        private static let queue = DispatchQueue(label: "com.bridgething.tunnel", attributes: .concurrent)
        private var connections: [UUID: NWConnection] = [:]
        private var pumps: [UUID: Task<Void, Never>] = [:]
        private let connectTimeout: Duration
        private let maxReadBytes = 64 * 1024

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
            for await (_, msg) in gateway.tunnel.data {
                await self?.handleData(msg)
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
        closeTask?.cancel(); closeTask = nil

        #if canImport(Network)
            for (_, pump) in pumps { pump.cancel() }
            pumps.removeAll()
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
            case let .failed(reason):
                conn.cancel()
                try? await handle.respondErr(TunnelErrorReply(error: .connectFailed(.init(reason: reason))))
            }
        #else
            try? await handle.respondErr(TunnelErrorReply(error: .unavailable))
        #endif
    }

    // MARK: - inbound: Data / Close (commands)

    private func handleData(_ msg: TunnelData) async {
        #if canImport(Network)
            // the daemon broadcasts to all peers; unknown tunnelId means this companion never opened that socket.
            guard let conn = connections[msg.tunnelId] else { return }
            conn.send(content: msg.bytes, completion: .contentProcessed { _ in })
        #endif
    }

    private func handleClose(_ msg: TunnelClosed) async {
        #if canImport(Network)
            teardown(msg.tunnelId)
        #endif
    }

    #if canImport(Network)
        // MARK: - byte pump (remote -> daemon)

        private func runPump(tunnelId: UUID, conn: NWConnection, gateway: BridgethingGateway) async {
            while !Task.isCancelled {
                let outcome = await receive(conn)
                if Task.isCancelled { return }
                if let data = outcome.data, !data.isEmpty {
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

        private func receive(_ conn: NWConnection) async -> ReceiveOutcome {
            let box = OneShotBox<ReceiveOutcome>()
            conn.receive(minimumIncompleteLength: 1, maximumLength: maxReadBytes) { data, _, isComplete, error in
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
    /// bridges a fire-once callback to a single continuation, tolerating either-order and dropping all but the first value.
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
