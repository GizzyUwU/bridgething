import Foundation

#if canImport(Network)
    import Network
#endif

public enum ConnectivityStatus: Sendable {
    case satisfied
    case unsatisfied
}

public protocol ConnectivityMonitoring: AnyObject, Sendable {
    func statuses() -> AsyncStream<ConnectivityStatus>
    func cancel()
}

public typealias ConnectivityMonitorFactory = @Sendable () -> any ConnectivityMonitoring

func makeDefaultConnectivityMonitor() -> any ConnectivityMonitoring {
    #if canImport(Network)
        return PathMonitorConnectivity()
    #else
        return NoOpConnectivityMonitor()
    #endif
}

#if canImport(Network)
    final class PathMonitorConnectivity: ConnectivityMonitoring, @unchecked Sendable {
        private let monitor = NWPathMonitor()
        private let queue = DispatchQueue(label: "com.bridgething.spotify.connectivity")

        func statuses() -> AsyncStream<ConnectivityStatus> {
            AsyncStream { continuation in
                monitor.pathUpdateHandler = { path in
                    continuation.yield(path.status == .satisfied ? .satisfied : .unsatisfied)
                }
                continuation.onTermination = { [monitor] _ in monitor.cancel() }
                monitor.start(queue: queue)
            }
        }

        func cancel() { monitor.cancel() }
    }
#else
    final class NoOpConnectivityMonitor: ConnectivityMonitoring, @unchecked Sendable {
        func statuses() -> AsyncStream<ConnectivityStatus> { AsyncStream { $0.finish() } }
        func cancel() {}
    }
#endif
