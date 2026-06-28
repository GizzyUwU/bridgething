import Logging
import os

/// swift-log handler that routes every record into the OS unified log (viewable
/// live in Console.app and Xcode). Bridges transitive swift-log producers
/// (nio / async-http-client in spotiny) onto os_log for free.
public struct OSLogHandler: LogHandler {
  public var logLevel: Logging.Logger.Level = .debug
  public var metadata: Logging.Logger.Metadata = [:]

  private let label: String
  private let osLog: os.Logger

  public init(label: String) {
    self.label = label
    self.osLog = os.Logger(subsystem: "com.bridgething", category: label)
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
    case .info, .notice: osLog.info("\(text, privacy: .public)")
    case .warning: osLog.warning("\(text, privacy: .public)")
    case .error, .critical: osLog.error("\(text, privacy: .public)")
    }
  }

  /// Composes the line: bare message, or message followed by metadata as
  /// sorted `key=value` pairs for stable output.
  static func render(_ message: Logging.Logger.Message, merged: Logging.Logger.Metadata) -> String {
    merged.isEmpty
      ? message.description
      : "\(message.description) \(merged.map { "\($0)=\($1)" }.sorted().joined(separator: " "))"
  }
}

/// Installs `OSLogHandler` as the process logging backend. Call once at app
/// launch before anything logs; swift-log traps on a second bootstrap.
public func bootstrapLogging() {
  LoggingSystem.bootstrap { OSLogHandler(label: $0) }
}
