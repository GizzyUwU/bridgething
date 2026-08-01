import Logging
import os

public struct OSLogHandler: LogHandler {
  public var logLevel: Logging.Logger.Level = .debug
  public var metadata: Logging.Logger.Metadata = [:]

  private let label: String
  private let osLog: os.Logger
  private let store: LogStore

  public init(label: String, store: LogStore = .shared) {
    self.label = label
    self.osLog = os.Logger(subsystem: "com.bridgething", category: label)
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

    switch level {
    case .trace, .debug: osLog.debug("\(text, privacy: .public)")
    case .info: osLog.info("\(text, privacy: .public)")
    case .notice: osLog.notice("\(text, privacy: .public)")
    case .warning: osLog.warning("\(text, privacy: .public)")
    case .error, .critical: osLog.error("\(text, privacy: .public)")
    }

    store.record(level: Self.storeLevel(level), label: label, message: text)
  }

  static func storeLevel(_ level: Logging.Logger.Level) -> LogStore.Level {
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

  static func render(_ message: Logging.Logger.Message, merged: Logging.Logger.Metadata) -> String {
    merged.isEmpty
      ? message.description
      : "\(message.description) \(merged.map { "\($0)=\($1)" }.sorted().joined(separator: " "))"
  }
}

public func bootstrapLogging() {
  LoggingSystem.bootstrap { OSLogHandler(label: $0) }
}
