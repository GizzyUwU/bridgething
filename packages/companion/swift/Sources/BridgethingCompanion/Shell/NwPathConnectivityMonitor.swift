#if canImport(Network)

    import BridgethingCompanionCore
    import Foundation
    import Network

    public final class NwPathConnectivityMonitor: ConnectivityMonitor, @unchecked Sendable {
        private let lock = NSLock()
        private var monitor: NWPathMonitor?

        public init() {}

        public func start(inbox: ConnectivityInbox) {
            stop()
            let fresh = NWPathMonitor()
            fresh.pathUpdateHandler = { path in
                inbox.onChanged(online: path.status == .satisfied)
            }
            lock.lock()
            monitor = fresh
            lock.unlock()
            fresh.start(queue: DispatchQueue(label: "com.bridgething.companion.connectivity"))
        }

        public func stop() {
            lock.lock()
            let held = monitor
            monitor = nil
            lock.unlock()
            held?.cancel()
        }
    }

    public final class UnmeteredTransferPolicy: TransferPolicy, @unchecked Sendable {
        private let monitor = NWPathMonitor()

        public init() {
            monitor.start(queue: DispatchQueue(label: "com.bridgething.companion.transfer-policy"))
        }

        deinit {
            monitor.cancel()
        }

        public func allowsLargeTransfer() -> Bool {
            let current = monitor.currentPath
            return current.status == .satisfied && !current.isExpensive && !current.isConstrained
        }
    }

#endif
