import Logging
import os

/// swift-log handler that tees every record into the diagnostics ring buffer and
/// the OS unified log
public struct DiagnosticsLogHandler: LogHandler {
  public var logLevel: Logging.Logger.Level = .debug
  public var metadata: Logging.Logger.Metadata = [:]

  private let label: String
  private let osLog: os.Logger

  public init(label: String) {
    self.label = label
    self.osLog = os.Logger(subsystem: "dev.bridgething", category: label)
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
    let text = merged.isEmpty
      ? message.description
      : "\(message.description) \(merged.map { "\($0)=\($1)" }.sorted().joined(separator: " "))"

    DiagnosticsBuffer.shared.recordLog(level: level.rawValue, target: label, message: text)

    switch level {
    case .trace, .debug: osLog.debug("\(text, privacy: .public)")
    case .info, .notice: osLog.info("\(text, privacy: .public)")
    case .warning: osLog.warning("\(text, privacy: .public)")
    case .error, .critical: osLog.error("\(text, privacy: .public)")
    }
  }
}

/// Installs `DiagnosticsLogHandler` as the process logging backend. Call once at
/// app launch before anything logs; swift-log traps on a second bootstrap.
public func bootstrapDiagnosticsLogging() {
  LoggingSystem.bootstrap { DiagnosticsLogHandler(label: $0) }
}
