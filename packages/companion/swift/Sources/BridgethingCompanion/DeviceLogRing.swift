import Foundation

public struct DeviceLogRecord: Sendable {
  public let seq: UInt64
  public let timestampMs: Double
  public let level: String
  public let message: String
}

/// Process-wide ring of device-log lines
public final class DeviceLogRing: @unchecked Sendable {
  public static let shared = DeviceLogRing()

  private let limit: Int
  private let lock = NSLock()
  private var records: [DeviceLogRecord] = []
  private var seqCounter: UInt64 = 0

  public init(limit: Int = 2000) {
    self.limit = limit
  }

  public func push(level: String, message: String) {
    lock.lock()
    seqCounter += 1
    records.append(
      DeviceLogRecord(
        seq: seqCounter,
        timestampMs: Date().timeIntervalSince1970 * 1000,
        level: level,
        message: message
      )
    )
    if records.count > limit { records.removeFirst(records.count - limit) }
    lock.unlock()
  }

  public func tail(limit: Int) -> [DeviceLogRecord] {
    lock.lock(); defer { lock.unlock() }
    guard limit < records.count else { return records }
    return Array(records.suffix(limit))
  }
}
