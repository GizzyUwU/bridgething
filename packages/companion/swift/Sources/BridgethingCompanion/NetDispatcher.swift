import BridgethingGateway
import BridgethingSchema
import Foundation
#if canImport(FoundationNetworking)
    import FoundationNetworking
#endif

/// Net surface implementation: subscribes to bridge → gateway Net traffic
/// (Fetch + WsOpen requests, WsClose / WsSend / StreamOpen / StreamCancel
/// commands) and answers with `URLSession`.
///
/// The streaming + websocket APIs are Apple-only (`URLSession.bytes(for:)`,
/// `URLSessionWebSocketTask`). On non-Apple platforms the dispatcher
/// surfaces "unavailable" errors and lets the daemon's webapp see
/// `NetError.unavailable`.
public actor NetDispatcher {
    private let urlSession: URLSession

    private var fetchTask: Task<Void, Never>?
    private var wsOpenTask: Task<Void, Never>?
    private var wsCloseTask: Task<Void, Never>?
    private var wsSendTask: Task<Void, Never>?
    private var streamOpenTask: Task<Void, Never>?
    private var streamCancelTask: Task<Void, Never>?

    #if canImport(Darwin)
        private var wsConnections: [UUID: URLSessionWebSocketTask] = [:]
        private var wsReceiveLoops: [UUID: Task<Void, Never>] = [:]
        private var streams: [UUID: Task<Void, Never>] = [:]
    #endif

    public init(urlSession: URLSession = .shared) {
        self.urlSession = urlSession
    }

    public func start(gateway: BridgethingGateway) async {
        fetchTask = Task { [weak self] in
            for await (handle, req) in gateway.net.fetchRequests {
                await self?.handleFetch(handle: handle, req: req.request)
            }
        }
        wsOpenTask = Task { [weak self] in
            for await (handle, req) in gateway.net.wsOpenRequests {
                await self?.handleWsOpen(handle: handle, req: req, gateway: gateway)
            }
        }
        wsCloseTask = Task { [weak self] in
            for await (_, msg) in gateway.net.wsClose {
                await self?.handleWsClose(connectionId: msg.connectionId, code: msg.code, reason: msg.reason)
            }
        }
        wsSendTask = Task { [weak self] in
            for await (_, msg) in gateway.net.wsSend {
                await self?.handleWsSend(connectionId: msg.connectionId, frame: msg.frame)
            }
        }
        streamOpenTask = Task { [weak self] in
            for await (_, msg) in gateway.net.streamOpen {
                await self?.handleStreamOpen(streamId: msg.streamId, req: msg.request, gateway: gateway)
            }
        }
        streamCancelTask = Task { [weak self] in
            for await (_, msg) in gateway.net.streamCancel {
                await self?.handleStreamCancel(streamId: msg.streamId)
            }
        }
    }

    public func stop() async {
        fetchTask?.cancel(); fetchTask = nil
        wsOpenTask?.cancel(); wsOpenTask = nil
        wsCloseTask?.cancel(); wsCloseTask = nil
        wsSendTask?.cancel(); wsSendTask = nil
        streamOpenTask?.cancel(); streamOpenTask = nil
        streamCancelTask?.cancel(); streamCancelTask = nil

        #if canImport(Darwin)
            for (_, task) in wsConnections {
                task.cancel(with: .normalClosure, reason: nil)
            }
            wsConnections.removeAll()
            for (_, loop) in wsReceiveLoops {
                loop.cancel()
            }
            wsReceiveLoops.removeAll()
            for (_, task) in streams {
                task.cancel()
            }
            streams.removeAll()
        #endif
    }

    // MARK: - Fetch

    private func handleFetch(handle: NetFetchRequestMsgHandle, req: NetFetchRequest) async {
        guard let url = URL(string: req.url) else {
            try? await handle.respondErr(NetFetchErrorReply(error: .requestFailed(.init(reason: "invalid url"))))
            return
        }
        var request = URLRequest(url: url)
        request.httpMethod = Self.methodString(req.method)
        for header in req.headers {
            request.addValue(header.value, forHTTPHeaderField: header.name)
        }
        if let body = req.body { request.httpBody = body }
        if let timeoutMs = req.timeoutMs { request.timeoutInterval = Double(timeoutMs) / 1000.0 }

        do {
            let (data, response) = try await urlSession.data(for: request)
            let http = response as? HTTPURLResponse
            let status = UInt16(http?.statusCode ?? 0)
            let headers = (http?.allHeaderFields ?? [:]).compactMap { k, v -> HttpHeader? in
                guard let key = k as? String, let val = v as? String else { return nil }
                return HttpHeader(name: key, value: val)
            }
            let resp = NetFetchResponse(status: status, headers: headers, body: data)
            try? await handle.respond(NetFetchReply(response: resp))
        } catch let urlErr as URLError where urlErr.code == .timedOut {
            try? await handle.respondErr(NetFetchErrorReply(error: .timeout))
        } catch {
            try? await handle.respondErr(NetFetchErrorReply(
                error: .requestFailed(.init(reason: error.localizedDescription))))
        }
    }

    // MARK: - WebSocket

    private func handleWsOpen(handle: NetWsOpenHandle, req: NetWsOpen, gateway: BridgethingGateway) async {
        #if canImport(Darwin)
            guard let url = URL(string: req.url) else {
                try? await handle.respondErr(NetWsErrorReply(error: .connectFailed(.init(reason: "invalid url"))))
                return
            }
            var request = URLRequest(url: url)
            for header in req.headers ?? [] {
                request.addValue(header.value, forHTTPHeaderField: header.name)
            }
            if let protocols = req.protocols, !protocols.isEmpty {
                request.setValue(protocols.joined(separator: ", "), forHTTPHeaderField: "Sec-WebSocket-Protocol")
            }
            let task: URLSessionWebSocketTask = urlSession.webSocketTask(with: request)
            let connId = req.connectionId
            wsConnections[connId] = task
            task.resume()

            try? await handle.respond(NetWsOpenReply(acceptedProtocol: nil))

            let loop = Task { [weak self] in
                guard let self else { return }
                await runWsReceive(connId: connId, task: task, gateway: gateway)
            }
            wsReceiveLoops[connId] = loop
        #else
            try? await handle.respondErr(NetWsErrorReply(error: .connectFailed(.init(reason: "websocket not supported on this platform"))))
        #endif
    }

    #if canImport(Darwin)
        private func runWsReceive(connId: UUID, task: URLSessionWebSocketTask, gateway: BridgethingGateway) async {
            while !Task.isCancelled {
                do {
                    let message = try await task.receive()
                    let frame: WsFrame = switch message {
                    case let .data(data): .binary(data)
                    case let .string(text): .text(text)
                    @unknown default: .text("")
                    }
                    try? await gateway.net.wsMessage(NetWsMessage(connectionId: connId, frame: frame))
                } catch {
                    let nsErr = error as NSError
                    let code = UInt16(truncatingIfNeeded: nsErr.code)
                    try? await gateway.net.wsClosed(NetWsClosed(
                        connectionId: connId, code: code, reason: error.localizedDescription
                    ))
                    await closeWs(connId: connId)
                    return
                }
            }
        }
    #endif

    private func handleWsClose(connectionId: UUID, code: UInt16?, reason: String?) async {
        #if canImport(Darwin)
            guard let task = wsConnections[connectionId] else { return }
            let closeCode = URLSessionWebSocketTask.CloseCode(rawValue: Int(code ?? 1000)) ?? .normalClosure
            task.cancel(with: closeCode, reason: reason?.data(using: .utf8))
            await closeWs(connId: connectionId)
        #endif
    }

    private func handleWsSend(connectionId: UUID, frame: WsFrame) async {
        #if canImport(Darwin)
            guard let task = wsConnections[connectionId] else { return }
            let message: URLSessionWebSocketTask.Message = switch frame {
            case let .text(s): .string(s)
            case let .binary(b): .data(b)
            }
            try? await task.send(message)
        #endif
    }

    #if canImport(Darwin)
        private func closeWs(connId: UUID) async {
            wsConnections.removeValue(forKey: connId)
            wsReceiveLoops.removeValue(forKey: connId)?.cancel()
        }
    #endif

    // MARK: - Stream

    private func handleStreamOpen(streamId: UUID, req: NetFetchRequest, gateway: BridgethingGateway) async {
        #if canImport(Darwin)
            let session = urlSession
            let task = Task { [weak self] in
                guard let self else { return }
                await runStream(streamId: streamId, req: req, session: session, gateway: gateway)
            }
            streams[streamId] = task
        #else
            try? await gateway.net.streamError(StreamError(
                streamId: streamId, error: .unavailable
            ))
        #endif
    }

    #if canImport(Darwin)
        private func runStream(streamId: UUID, req: NetFetchRequest, session: URLSession, gateway: BridgethingGateway) async {
            guard let url = URL(string: req.url) else {
                try? await gateway.net.streamError(StreamError(
                    streamId: streamId, error: .requestFailed(.init(reason: "invalid url"))
                ))
                await streamFinished(id: streamId)
                return
            }
            var request = URLRequest(url: url)
            request.httpMethod = Self.methodString(req.method)
            for header in req.headers {
                request.addValue(header.value, forHTTPHeaderField: header.name)
            }
            if let body = req.body { request.httpBody = body }

            do {
                let (bytes, response) = try await session.bytes(for: request)
                let http = response as? HTTPURLResponse
                let status = UInt16(http?.statusCode ?? 0)
                let headers = (http?.allHeaderFields ?? [:]).compactMap { k, v -> HttpHeader? in
                    guard let key = k as? String, let val = v as? String else { return nil }
                    return HttpHeader(name: key, value: val)
                }
                let totalSize: UInt32? = {
                    guard let len = http?.expectedContentLength, len >= 0 else { return nil }
                    return UInt32(clamping: len)
                }()
                try? await gateway.net.streamBegin(StreamBegin(
                    streamId: streamId, status: status, headers: headers, totalSize: totalSize
                ))

                var offset: UInt32 = 0
                var buffer = Data()
                buffer.reserveCapacity(8192)
                for try await byte in bytes {
                    if Task.isCancelled { return }
                    buffer.append(byte)
                    if buffer.count >= 8192 {
                        try? await gateway.net.streamChunk(StreamChunk(
                            streamId: streamId, offset: offset, bytes: buffer
                        ))
                        offset = offset &+ UInt32(buffer.count)
                        buffer.removeAll(keepingCapacity: true)
                    }
                }
                if !buffer.isEmpty {
                    try? await gateway.net.streamChunk(StreamChunk(
                        streamId: streamId, offset: offset, bytes: buffer
                    ))
                }
                try? await gateway.net.streamEnd(StreamEnd(streamId: streamId))
            } catch let urlErr as URLError where urlErr.code == .timedOut {
                try? await gateway.net.streamError(StreamError(streamId: streamId, error: .timeout))
            } catch {
                try? await gateway.net.streamError(StreamError(
                    streamId: streamId, error: .requestFailed(.init(reason: error.localizedDescription))
                ))
            }
            await streamFinished(id: streamId)
        }

        private func streamFinished(id: UUID) async {
            streams.removeValue(forKey: id)
        }
    #endif

    private func handleStreamCancel(streamId: UUID) async {
        #if canImport(Darwin)
            if let task = streams[streamId] {
                task.cancel()
                streams.removeValue(forKey: streamId)
            }
        #endif
    }

    // MARK: - helpers

    private static func methodString(_ method: HttpMethod) -> String {
        switch method {
        case .get: "GET"
        case .head: "HEAD"
        case .post: "POST"
        case .put: "PUT"
        case .patch: "PATCH"
        case .delete: "DELETE"
        case .options: "OPTIONS"
        }
    }
}
