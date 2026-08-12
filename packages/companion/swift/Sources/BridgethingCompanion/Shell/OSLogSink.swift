#if canImport(Darwin)

    import BridgethingCompanionCore
    import Foundation
    import os

    public final class OSLogSink: LogSink, @unchecked Sendable {
        private let subsystem: String

        public init(subsystem: String = "com.bridgething") {
            self.subsystem = subsystem
        }

        public func onLine(level: LogLevel, target: String, message: String) {
            let logger = Logger(subsystem: subsystem, category: target)
            switch level {
            case .trace: logger.debug("\(message, privacy: .public)")
            case .debug: logger.debug("\(message, privacy: .public)")
            case .info: logger.notice("\(message, privacy: .public)")
            case .warn: logger.warning("\(message, privacy: .public)")
            case .error: logger.error("\(message, privacy: .public)")
            }
            CompanionLogs.shared.store?.record(level: Self.storeLevel(level), label: target, message: message)
            CompanionLogRelay.shared.push(level: level, target: target, message: message)
        }

        private static func storeLevel(_ level: LogLevel) -> LogStoreLevel {
            switch level {
            case .trace: .trace
            case .debug: .debug
            case .info: .info
            case .warn: .warn
            case .error: .error
            }
        }
    }

#endif
