import Foundation
import Spotify

#if canImport(Darwin)
    final class UrlSessionWsTransport: WsTransport, @unchecked Sendable {
        private let session: URLSession
        private let lock = NSLock()
        private var task: URLSessionWebSocketTask?
        private var generation: UInt64 = 0

        init() {
            let cfg = URLSessionConfiguration.default
            cfg.shouldUseExtendedBackgroundIdleMode = true
            cfg.waitsForConnectivity = true
            cfg.networkServiceType = .responsiveData
            cfg.timeoutIntervalForRequest = 60
            cfg.timeoutIntervalForResource = TimeInterval(Int32.max)
            session = URLSession(configuration: cfg)
        }

        func connect(url: String, inbox: WsInbox) {
            guard let parsed = URL(string: url) else {
                inbox.onClosed(reason: "invalid url")
                return
            }
            let task = session.webSocketTask(with: parsed)
            let gen: UInt64 = lock.withLock {
                generation &+= 1
                self.task?.cancel(with: .goingAway, reason: nil)
                self.task = task
                return generation
            }
            task.resume()
            receive(task: task, inbox: inbox, gen: gen)
        }

        private func receive(task: URLSessionWebSocketTask, inbox: WsInbox, gen: UInt64) {
            task.receive { [weak self] result in
                guard let self else { return }
                guard self.lock.withLock({ self.generation == gen }) else { return }
                switch result {
                case let .success(message):
                    switch message {
                    case let .string(text): inbox.onText(text: text)
                    case .data: break // dealer is json over text frames; binary is ignored
                    @unknown default: break
                    }
                    self.receive(task: task, inbox: inbox, gen: gen)
                case let .failure(error):
                    inbox.onClosed(reason: error.localizedDescription)
                }
            }
        }

        func sendText(text: String) {
            let task = lock.withLock { self.task }
            task?.send(.string(text)) { _ in }
        }

        func disconnect() {
            lock.withLock {
                generation &+= 1
                task?.cancel(with: .normalClosure, reason: nil)
                task = nil
            }
        }
    }
#endif
