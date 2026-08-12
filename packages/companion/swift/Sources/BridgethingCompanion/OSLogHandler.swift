import BridgethingCompanionCore
import Foundation
import Logging
#if canImport(os)
  import os
#endif

public final class CompanionLogRelay: @unchecked Sendable {
  public static let shared = CompanionLogRelay()

  private let lock = NSLock()
  private var inbox: LogInbox?

  private init() {}

  public func setInbox(_ next: LogInbox?) {
    lock.lock()
    inbox = next
    lock.unlock()
  }

  public func push(level: BridgethingCompanionCore.LogLevel, target: String, message: String) {
    lock.lock()
    let held = inbox
    lock.unlock()
    held?.push(level: level, target: target, message: message)
  }
}

public struct OSLogHandler: LogHandler {
  public var logLevel: Logging.Logger.Level = .debug
  public var metadata: Logging.Logger.Metadata = [:]

  private let label: String
  #if canImport(os)
    private let osLog: os.Logger
  #endif
  private let store: LogStore?

  public init(label: String, store: LogStore? = nil) {
    self.label = label
    #if canImport(os)
      self.osLog = os.Logger(subsystem: "com.bridgething", category: label)
    #endif
    self.store = store
  }

  public subscript(metadataKey key: String) -> Logging.Logger.Metadata.Value? {
    get { metadata[key] }
    set { metadata[key] = newValue }
  }

  public func log(
    level: Logging.Logger.Level,
    message: Logging.Logger.Message,
    metadata explicit: Logging.Logger.Metadata?,
    source _: String,
    file _: String,
    function _: String,
    line _: UInt
  ) {
    let merged = explicit.map { metadata.merging($0) { _, new in new } } ?? metadata
    let text = Self.render(message, merged: merged)

    #if canImport(os)
      switch level {
      case .trace, .debug: osLog.debug("\(text, privacy: .public)")
      case .info: osLog.info("\(text, privacy: .public)")
      case .notice: osLog.notice("\(text, privacy: .public)")
      case .warning: osLog.warning("\(text, privacy: .public)")
      case .error, .critical: osLog.error("\(text, privacy: .public)")
      }
    #else
      print("[\(level)] \(label): \(text)")
    #endif

    (store ?? CompanionLogs.shared.store)?.record(level: Self.storeLevel(level), label: label, message: text)
    CompanionLogRelay.shared.push(level: Self.coreLevel(level), target: label, message: text)
  }

  static func storeLevel(_ level: Logging.Logger.Level) -> LogStoreLevel {
    switch level {
    case .trace: .trace
    case .debug: .debug
    case .info: .info
    case .notice: .notice
    case .warning: .warn
    case .error: .error
    case .critical: .fatal
    }
  }

  static func coreLevel(_ level: Logging.Logger.Level) -> BridgethingCompanionCore.LogLevel {
    switch level {
    case .trace: .trace
    case .debug: .debug
    case .info, .notice: .info
    case .warning: .warn
    case .error, .critical: .error
    }
  }

  static func render(_ message: Logging.Logger.Message, merged: Logging.Logger.Metadata) -> String {
    merged.isEmpty
      ? message.description
      : "\(message.description) \(merged.map { "\($0)=\($1)" }.sorted().joined(separator: " "))"
  }
}

public func bootstrapLogging() {
  LoggingSystem.bootstrap { OSLogHandler(label: $0) }
}
