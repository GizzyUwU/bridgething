import Foundation

/// One record in the diagnostics ring buffer
public struct DiagRecord: Sendable {
  public enum Kind: Sendable { case frame, log, breadcrumb }
  public enum Direction: Sendable { case outbound, inbound }
  public enum FrameKind: Sendable { case request, response, event, command }

  public let seq: UInt64
  public let timestampMs: Double
  public let kind: Kind

  public let deviceId: String?
  public let direction: Direction?
  public let frameKind: FrameKind?
  public let surface: String?
  public let byteSize: Int?
  public let requestId: String?
  public let latencyMs: Double?

  public let level: String?
  public let target: String?
  public let message: String?

  public let category: String?
  public let detail: String?
  public let fields: [(key: String, value: String)]?

  public let payload: String?

  var approxBytes: Int {
    var n = 96
    n += (deviceId?.utf8.count ?? 0) + (surface?.utf8.count ?? 0) + (requestId?.utf8.count ?? 0)
    n += (level?.utf8.count ?? 0) + (target?.utf8.count ?? 0) + (message?.utf8.count ?? 0)
    n += (category?.utf8.count ?? 0) + (detail?.utf8.count ?? 0) + (payload?.utf8.count ?? 0)
    for f in fields ?? [] { n += f.key.utf8.count + f.value.utf8.count + 16 }
    return n
  }
}

/// Process-wide, always-on diagnostics ring buffer: wire frames (from the gateway
/// send/ingest tap), companion native log lines (from the log-facade tee), and
/// structured breadcrumbs (from the glue augmentation path). FIFO eviction keeps
/// it under a byte budget. A shared singleton because the three producers are
/// bootstrapped at process scope and the host reads/subscribes once; mirrors the
/// existing process-wide bridge registries.
///
/// Lock-based rather than an actor so the per-frame `record*` calls stay
/// synchronous on the gateway's hot path (an actor hop would spawn a Task per
/// frame); the AsyncStream fan-out matches `EventBroadcaster`.
public final class DiagnosticsBuffer: @unchecked Sendable {
  public static let shared = DiagnosticsBuffer()

  private let byteBudget: Int
  private let lock = NSLock()
  private var records: [DiagRecord] = []
  private var bytes = 0
  private var seqCounter: UInt64 = 0
  private var subscribers: [UUID: AsyncStream<DiagRecord>.Continuation] = [:]

  public init(byteBudget: Int = 8 * 1024 * 1024) {
    self.byteBudget = byteBudget
  }

  /// Live stream of records inserted after subscription. The foreground pull uses
  /// `tail(limit:)`; this carries everything after that point. No replay cache:
  /// the consumer reconciles via `seq` against its last-seen tail.
  public nonisolated var stream: AsyncStream<DiagRecord> {
    AsyncStream { continuation in
      let id = UUID()
      lock.lock()
      subscribers[id] = continuation
      lock.unlock()
      continuation.onTermination = { [weak self] _ in
        guard let self else { return }
        lock.lock()
        subscribers.removeValue(forKey: id)
        lock.unlock()
      }
    }
  }

  /// Most-recent `limit` records, oldest-first.
  public func tail(limit: Int) -> [DiagRecord] {
    lock.lock(); defer { lock.unlock() }
    guard limit < records.count else { return records }
    return Array(records.suffix(limit))
  }

  public func recordFrame(
    deviceId: String,
    direction: DiagRecord.Direction,
    frameKind: DiagRecord.FrameKind,
    surface: String,
    byteSize: Int,
    requestId: String?,
    latencyMs: Double?,
    payload: String?
  ) {
    insert { seq, ts in
      DiagRecord(
        seq: seq, timestampMs: ts, kind: .frame,
        deviceId: deviceId, direction: direction, frameKind: frameKind,
        surface: surface, byteSize: byteSize, requestId: requestId, latencyMs: latencyMs,
        level: nil, target: nil, message: nil,
        category: nil, detail: nil, fields: nil,
        payload: payload
      )
    }
  }

  public func recordLog(level: String, target: String?, message: String) {
    insert { seq, ts in
      DiagRecord(
        seq: seq, timestampMs: ts, kind: .log,
        deviceId: nil, direction: nil, frameKind: nil,
        surface: nil, byteSize: nil, requestId: nil, latencyMs: nil,
        level: level, target: target, message: message,
        category: nil, detail: nil, fields: nil,
        payload: nil
      )
    }
  }

  public func recordBreadcrumb(
    category: String,
    detail: String,
    fields: [(key: String, value: String)]? = nil
  ) {
    insert { seq, ts in
      DiagRecord(
        seq: seq, timestampMs: ts, kind: .breadcrumb,
        deviceId: nil, direction: nil, frameKind: nil,
        surface: nil, byteSize: nil, requestId: nil, latencyMs: nil,
        level: nil, target: nil, message: nil,
        category: category, detail: detail, fields: fields,
        payload: nil
      )
    }
  }

  private func insert(_ build: (UInt64, Double) -> DiagRecord) {
    lock.lock()
    seqCounter += 1
    let record = build(seqCounter, Date().timeIntervalSince1970 * 1000)
    records.append(record)
    bytes += record.approxBytes
    while bytes > byteBudget, records.count > 1 {
      bytes -= records.removeFirst().approxBytes
    }
    let copies = Array(subscribers.values)
    lock.unlock()
    for c in copies { c.yield(record) }
  }
}
