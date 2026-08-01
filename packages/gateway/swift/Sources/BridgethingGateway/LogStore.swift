import Foundation

public final class LogStore: @unchecked Sendable {
  public static let shared = LogStore()

  public enum Level: Character, Sendable {
    case trace = "V"
    case debug = "D"
    case info = "I"
    case notice = "N"
    case warn = "W"
    case error = "E"
    case fatal = "F"

    var pins: Bool { self == .error || self == .fatal }
  }

  public struct Limits: Sendable {
    public var launches: Int
    public var segmentsPerLaunch: Int
    public var segmentBytes: Int64
    public var queueCapacity: Int

    public var pinnedBytesLimit: Int64

    public init(
      launches: Int = 3,
      segmentsPerLaunch: Int = 2,
      segmentBytes: Int64 = 512 * 1024,
      queueCapacity: Int = 4096,
      pinnedBytesLimit: Int64 = 32 * 1024 * 1024
    ) {
      self.launches = launches
      self.segmentsPerLaunch = segmentsPerLaunch
      self.segmentBytes = segmentBytes
      self.queueCapacity = queueCapacity
      self.pinnedBytesLimit = pinnedBytesLimit
    }
  }

  public struct Archive: Sendable, Equatable {
    public let id: String
    public let startedAtMs: Int64
    public let bytes: Int64
    public let pinned: Bool
    public let current: Bool
  }

  private struct Entry {
    let line: String
    let pins: Bool
  }

  private static let pinSuffix = ".keep"
  private static let segmentSuffix = ".log"

  private let limits: Limits
  private let fm = FileManager.default

  private let queue = NSCondition()
  private var pending: [Entry] = []
  private var running = false
  private var draining = false

  private var dropped: Int64 = 0

  private var sinkGeneration = 0

  private let state = NSLock()
  private var root: URL?
  private var launchName: String?
  private var thread: Thread?

  public init(limits: Limits = Limits()) {
    self.limits = limits
  }

  // MARK: - install

  public static func defaultRoot() -> URL {
    let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
      ?? FileManager.default.temporaryDirectory
    return base.appendingPathComponent("bridgething-logs", isDirectory: true)
  }

  public func install(root url: URL = LogStore.defaultRoot()) {
    state.lock()
    defer { state.unlock() }
    guard thread == nil else { return }

    try? fm.createDirectory(at: url, withIntermediateDirectories: true)
    root = url

    let name = String(Int64(Date().timeIntervalSince1970 * 1000))
    let launch = url.appendingPathComponent(name, isDirectory: true)
    try? fm.createDirectory(at: launch, withIntermediateDirectories: true)
    launchName = name
    pruneLaunches(in: url)

    queue.lock()
    running = true
    queue.unlock()

    let worker = Thread { [weak self] in self?.runWriter(launch: launch) }
    worker.name = "bridgething-logstore"
    worker.qualityOfService = .background
    thread = worker
    worker.start()
  }

  // MARK: - ingest

  public func record(level: Level, label: String, message: String) {
    queue.lock()
    let live = running
    queue.unlock()
    guard live else { return }

    let prefix = "\(Self.stamp(Date())) \(Self.pid) \(Self.currentThreadId()) \(level.rawValue) \(label): "
    queue.lock()
    defer { queue.unlock() }
    for part in message.split(separator: "\n", omittingEmptySubsequences: false) {
      if pending.count >= limits.queueCapacity {
        dropped += 1
        continue
      }
      pending.append(Entry(line: prefix + part, pins: level.pins))
    }
    queue.broadcast()
  }

  // MARK: - export

  public func archives() -> [Archive] {
    let (base, live) = currentLayout()
    guard let base else { return [] }
    return Self.launchDirs(in: base)
      .map { dir in
        Archive(
          id: dir.lastPathComponent,
          startedAtMs: Int64(dir.lastPathComponent) ?? 0,
          bytes: Self.totalBytes(Self.segments(in: dir)),
          pinned: !Self.pinnedSegments(in: dir).isEmpty,
          current: dir.lastPathComponent == live
        )
      }
      .sorted { $0.startedAtMs > $1.startedAtMs }
  }

  public func retainedBytes() -> Int64 {
    let (base, _) = currentLayout()
    guard let base else { return 0 }
    return Self.launchDirs(in: base).reduce(0) { $0 + Self.totalBytes(Self.segments(in: $1)) }
  }

  public func delete(id: String) {
    state.lock()
    defer { state.unlock() }
    guard let base = root, Self.isLaunchId(id) else { return }
    let dir = base.appendingPathComponent(id, isDirectory: true)
    guard Self.isDirectory(dir) else { return }
    flush()
    if id == launchName {
      truncate(dir)
      bumpGeneration()
    } else {
      try? fm.removeItem(at: dir)
    }
  }

  public func clear() {
    state.lock()
    defer { state.unlock() }
    guard let base = root else { return }
    flush()
    for dir in Self.launchDirs(in: base) {
      if dir.lastPathComponent == launchName {
        truncate(dir)
      } else {
        try? fm.removeItem(at: dir)
      }
    }
    bumpGeneration()
    queue.lock()
    dropped = 0
    queue.unlock()
  }

  @discardableResult
  public func exportTo(_ target: URL, id: String? = nil) throws -> URL {
    flush()
    try fm.createDirectory(at: target.deletingLastPathComponent(), withIntermediateDirectories: true)
    fm.createFile(atPath: target.path, contents: nil)
    let out = try FileHandle(forWritingTo: target)
    defer { try? out.close() }

    let (base, live) = currentLayout()
    var dirs = base.map { Self.launchDirs(in: $0) } ?? []
    if let id { dirs = dirs.filter { $0.lastPathComponent == id } }

    queue.lock()
    let lost = dropped
    queue.unlock()

    var header = "bridgething log export\n"
    header += "generated: \(Date())\n"
    header += "launches: \(dirs.count)\n"
    if lost > 0 { header += "dropped lines (writer backpressure): \(lost)\n" }
    header += "\n"
    try out.write(contentsOf: Data(header.utf8))

    for dir in dirs {
      let name = dir.lastPathComponent
      let stamp = Int64(name).map { "\(Date(timeIntervalSince1970: Double($0) / 1000))" } ?? name
      let current = name == live ? " (current)" : ""
      let pinned = Self.pinnedSegments(in: dir).isEmpty ? "" : " [pinned: contains errors]"
      try out.write(contentsOf: Data("===== launch \(stamp)\(current)\(pinned) =====\n".utf8))
      for segment in Self.segments(in: dir) {
        do {
          let body = try Data(contentsOf: segment, options: .mappedIfSafe)
          try out.write(contentsOf: body)
          if body.last != 0x0A { try out.write(contentsOf: Data([0x0A])) }
        } catch {
          let note = "<<unreadable segment \(segment.lastPathComponent): \(error.localizedDescription)>>\n"
          try out.write(contentsOf: Data(note.utf8))
        }
      }
      try out.write(contentsOf: Data("\n".utf8))
    }
    return target
  }

  public func flush() {
    queue.lock()
    defer { queue.unlock() }
    let deadline = Date().addingTimeInterval(2)
    while !pending.isEmpty || draining {
      guard Date() < deadline else { return }
      queue.wait(until: deadline)
    }
  }

  // MARK: - writer thread

  private func runWriter(launch: URL) {
    var sink: Sink?
    var generation = 0
    while true {
      queue.lock()
      while pending.isEmpty { queue.wait() }
      let batch = pending
      pending.removeAll(keepingCapacity: true)
      draining = true
      let wanted = sinkGeneration
      queue.unlock()

      if wanted != generation {
        sink?.close()
        sink = nil
        generation = wanted
      }

      do {
        for entry in batch {
          let active: Sink
          if let existing = sink {
            active = existing
          } else {
            active = try openSink(in: launch)
            sink = active
          }
          active.write(entry.line, pins: entry.pins)
          if active.bytes >= limits.segmentBytes {
            if active.sawError { active.pin() }
            active.close()
            sink = nil
          }
        }
        if let active = sink {
          if active.sawError { active.pin() }
          try active.flush()
        }
      } catch {
        sink?.close()
        sink = nil
      }

      queue.lock()
      draining = false
      queue.broadcast()
      queue.unlock()
    }
  }

  private func openSink(in launch: URL) throws -> Sink {
    let existing = Self.segments(in: launch)
    let newest = existing.last
    let target: URL
    if let newest, Self.fileSize(newest) < limits.segmentBytes {
      target = newest
    } else {
      let next = (newest.map { Self.segmentIndex($0) } ?? -1) + 1
      target = launch.appendingPathComponent(String(format: "%04d", next) + Self.segmentSuffix)
    }
    pruneSegments(in: launch, keepAlso: target)
    return try Sink(url: target)
  }

  private final class Sink {
    private(set) var bytes: Int64

    private(set) var sawError = false

    private let url: URL
    private let handle: FileHandle
    private var pinned = false
    private var buffer = Data()

    init(url: URL) throws {
      self.url = url
      if !FileManager.default.fileExists(atPath: url.path) {
        FileManager.default.createFile(atPath: url.path, contents: nil)
      }
      handle = try FileHandle(forWritingTo: url)
      let end = try handle.seekToEnd()
      bytes = Int64(end)
    }

    func write(_ line: String, pins: Bool) {
      let utf8 = Data(line.utf8)
      buffer.append(utf8)
      buffer.append(0x0A)
      bytes += Int64(utf8.count + 1)
      if !pinned, pins { sawError = true }
    }

    func pin() {
      sawError = false
      guard !pinned else { return }
      let marker = LogStore.pinMarker(for: url)
      pinned = (try? Data().write(to: marker)) != nil || FileManager.default.fileExists(atPath: marker.path)
    }

    func flush() throws {
      guard !buffer.isEmpty else { return }
      try handle.write(contentsOf: buffer)
      buffer.removeAll(keepingCapacity: true)
    }

    func close() {
      try? flush()
      try? handle.close()
    }
  }

  // MARK: - rotation

  private func pruneLaunches(in base: URL) {
    let dirs = Self.launchDirs(in: base)
    let pinned = dirs.filter { !Self.pinnedSegments(in: $0).isEmpty }
    let rotating = dirs.filter { Self.pinnedSegments(in: $0).isEmpty }

    let excess = rotating.count - limits.launches
    if excess > 0 {
      for dir in rotating.prefix(excess) { try? fm.removeItem(at: dir) }
    }

    var total = pinned.reduce(Int64(0)) { $0 + Self.totalBytes(Self.pinnedSegments(in: $1)) }
    for dir in pinned {
      if total <= limits.pinnedBytesLimit { break }
      total -= Self.totalBytes(Self.pinnedSegments(in: dir))
      try? fm.removeItem(at: dir)
    }
  }

  private func pruneSegments(in launch: URL, keepAlso: URL) {
    var all = Self.segments(in: launch)
    if !all.contains(where: { $0.lastPathComponent == keepAlso.lastPathComponent }) { all.append(keepAlso) }
    all.sort { Self.segmentIndex($0) < Self.segmentIndex($1) }
    let rotating = all.filter { !Self.isPinned($0) }
    let excess = rotating.count - limits.segmentsPerLaunch
    guard excess > 0 else { return }
    for dir in rotating.prefix(excess) where dir.lastPathComponent != keepAlso.lastPathComponent {
      try? fm.removeItem(at: dir)
    }
  }

  private func truncate(_ dir: URL) {
    for segment in Self.segments(in: dir) {
      try? fm.removeItem(at: Self.pinMarker(for: segment))
      try? fm.removeItem(at: segment)
    }
  }

  private func bumpGeneration() {
    queue.lock()
    sinkGeneration += 1
    queue.unlock()
  }

  private func currentLayout() -> (URL?, String?) {
    state.lock()
    defer { state.unlock() }
    return (root, launchName)
  }

  // MARK: - layout helpers

  private static func isLaunchId(_ id: String) -> Bool {
    !id.isEmpty && id.allSatisfy(\.isASCII) && id.allSatisfy(\.isNumber)
  }

  private static func isDirectory(_ url: URL) -> Bool {
    var isDir: ObjCBool = false
    return FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir) && isDir.boolValue
  }

  private static func launchDirs(in base: URL) -> [URL] {
    let entries = (try? FileManager.default.contentsOfDirectory(at: base, includingPropertiesForKeys: nil)) ?? []
    return entries
      .filter { isLaunchId($0.lastPathComponent) && isDirectory($0) }
      .sorted { $0.lastPathComponent < $1.lastPathComponent }
  }

  private static func segments(in dir: URL) -> [URL] {
    let entries = (try? FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: nil)) ?? []
    return entries
      .filter { $0.lastPathComponent.hasSuffix(segmentSuffix) }
      .sorted { segmentIndex($0) < segmentIndex($1) }
  }

  private static func segmentIndex(_ url: URL) -> Int {
    Int(url.lastPathComponent.replacingOccurrences(of: segmentSuffix, with: "")) ?? -1
  }

  private static func pinMarker(for segment: URL) -> URL {
    let base = segment.lastPathComponent.replacingOccurrences(of: segmentSuffix, with: "")
    return segment.deletingLastPathComponent().appendingPathComponent(base + pinSuffix)
  }

  private static func isPinned(_ segment: URL) -> Bool {
    FileManager.default.fileExists(atPath: pinMarker(for: segment).path)
  }

  private static func pinnedSegments(in dir: URL) -> [URL] {
    segments(in: dir).filter { isPinned($0) }
  }

  private static func fileSize(_ url: URL) -> Int64 {
    let attrs = try? FileManager.default.attributesOfItem(atPath: url.path)
    return (attrs?[.size] as? NSNumber)?.int64Value ?? 0
  }

  private static func totalBytes(_ urls: [URL]) -> Int64 {
    urls.reduce(0) { $0 + fileSize($1) }
  }

  // MARK: - line prefix

  private static let pid = ProcessInfo.processInfo.processIdentifier

  private static func currentThreadId() -> UInt64 {
    var id: UInt64 = 0
    pthread_threadid_np(nil, &id)
    return id
  }

  private static let stampLock = NSLock()
  private static let stampFormatter: DateFormatter = {
    let f = DateFormatter()
    f.locale = Locale(identifier: "en_US_POSIX")
    f.dateFormat = "MM-dd HH:mm:ss.SSS"
    return f
  }()

  static func stamp(_ date: Date, timeZone: TimeZone? = nil) -> String {
    stampLock.lock()
    defer { stampLock.unlock() }
    let previous = stampFormatter.timeZone
    if let timeZone { stampFormatter.timeZone = timeZone }
    defer { stampFormatter.timeZone = previous }
    return stampFormatter.string(from: date)
  }
}
