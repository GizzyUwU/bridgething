import BridgethingGateway
import Foundation
import os
import Spotify

#if canImport(Darwin)
    final class OsLogSink: LogSink, @unchecked Sendable {
        func log(level: String, target: String, message: String) {
            let logger = Logger(subsystem: "com.bridgething.spotify", category: target)
            let line = "[\(level)] \(message)"
            switch level {
            case "ERROR": logger.error("\(line, privacy: .public)")
            case "WARN": logger.warning("\(line, privacy: .public)")
            default: logger.notice("\(line, privacy: .public)")
            }

            LogStore.shared.record(level: Self.storeLevel(level), label: target, message: message)
            LocalLogRelay.shared.push(level: level, target: target, message: message)
        }

        private static func storeLevel(_ level: String) -> LogStore.Level {
            switch level {
            case "ERROR": .error
            case "WARN": .warn
            case "INFO": .info
            default: .debug
            }
        }
    }
#endif
