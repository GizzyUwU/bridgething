import Foundation
import XCTest
@testable import BridgethingGateway

final class LogStoreTests: XCTestCase {
  private var root: URL!

  override func setUpWithError() throws {
    root = FileManager.default.temporaryDirectory
      .appendingPathComponent("logstore-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
  }

  override func tearDownWithError() throws {
    try? FileManager.default.removeItem(at: root)
  }

  // MARK: - helpers

  private func makeStore(
    launches: Int = 3,
    segmentsPerLaunch: Int = 2,
    segmentBytes: Int64 = 1024,
    queueCapacity: Int = 4096,
    pinnedBytesLimit: Int64 = 32 * 1024 * 1024
  ) -> LogStore {
    LogStore(
      limits: .init(
        launches: launches,
        segmentsPerLaunch: segmentsPerLaunch,
        segmentBytes: segmentBytes,
        queueCapacity: queueCapacity,
        pinnedBytesLimit: pinnedBytesLimit
      )
    )
  }

  @discardableResult
  private func seedLaunch(id: String, segments: [(index: Int, body: String, pinned: Bool)]) throws -> URL {
    let dir = root.appendingPathComponent(id, isDirectory: true)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    for segment in segments {
      let name = String(format: "%04d", segment.index)
      try Data(segment.body.utf8).write(to: dir.appendingPathComponent("\(name).log"))
      if segment.pinned { try Data().write(to: dir.appendingPathComponent("\(name).keep")) }
    }
    return dir
  }

  private func liveLaunch(_ store: LogStore) throws -> URL {
    let id = try XCTUnwrap(store.archives().first { $0.current }?.id)
    return root.appendingPathComponent(id, isDirectory: true)
  }

  private func segmentNames(in dir: URL) -> [String] {
    let entries = (try? FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: nil)) ?? []
    return entries.map(\.lastPathComponent).filter { $0.hasSuffix(".log") }.sorted()
  }

  private func exportText(_ store: LogStore, id: String? = nil) throws -> String {
    let target = root.appendingPathComponent("bundle-\(UUID().uuidString).txt")
    try store.exportTo(target, id: id)
    return try String(contentsOf: target, encoding: .utf8)
  }

  // MARK: - line shape

  func testRecordPrefixesEveryLineWithLevelAndLabel() throws {
    let store = makeStore()
    store.install(root: root)
    store.record(level: .info, label: "gateway", message: "hello")
    store.flush()

    let text = try exportText(store)
    let line = try XCTUnwrap(text.split(separator: "\n").first { $0.contains("hello") })
    XCTAssertNotNil(
      line.range(of: #"^\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3} +\d+ +\d+ I gateway: hello$"#, options: .regularExpression),
      "unexpected line shape: \(line)"
    )
  }

  func testMultiLineMessageBecomesOnePrefixedLineEach() throws {
    let store = makeStore()
    store.install(root: root)
    store.record(level: .warn, label: "gateway", message: "first\nsecond")
    store.flush()

    let text = try exportText(store)
    XCTAssertEqual(text.components(separatedBy: " W gateway: ").count - 1, 2)
    XCTAssertTrue(text.contains(" W gateway: first"))
    XCTAssertTrue(text.contains(" W gateway: second"))
  }

  func testRecordBeforeInstallIsDropped() throws {
    let store = makeStore()
    store.record(level: .info, label: "gateway", message: "ignored")
    XCTAssertEqual(store.retainedBytes(), 0)
    XCTAssertTrue(store.archives().isEmpty)
  }

  // MARK: - segment ring

  func testSegmentRollsOverAtTheByteCap() throws {
    let store = makeStore(segmentBytes: 512)
    store.install(root: root)
    let dir = try liveLaunch(store)

    for i in 0 ..< 40 { store.record(level: .info, label: "t", message: "line \(i) \(String(repeating: "x", count: 40))") }
    store.flush()

    XCTAssertGreaterThan(segmentNames(in: dir).count, 1)
  }

  func testSegmentRingKeepsOnlyTheNewestSegments() throws {
    let store = makeStore(segmentsPerLaunch: 2, segmentBytes: 256)
    store.install(root: root)
    let dir = try liveLaunch(store)

    for i in 0 ..< 200 { store.record(level: .info, label: "t", message: "line \(i) \(String(repeating: "x", count: 60))") }
    store.flush()

    XCTAssertEqual(segmentNames(in: dir).count, 2)
    XCTAssertTrue(try exportText(store).contains("line 199"))
  }

  func testErrorSegmentIsPinnedAndSurvivesTheSegmentRing() throws {
    let store = makeStore(segmentsPerLaunch: 1, segmentBytes: 256)
    store.install(root: root)
    let dir = try liveLaunch(store)

    store.record(level: .error, label: "t", message: "the thing that went wrong")
    store.flush()
    XCTAssertTrue(FileManager.default.fileExists(atPath: dir.appendingPathComponent("0000.keep").path))

    for i in 0 ..< 200 { store.record(level: .info, label: "t", message: "line \(i) \(String(repeating: "x", count: 60))") }
    store.flush()

    XCTAssertTrue(segmentNames(in: dir).contains("0000.log"))
    XCTAssertTrue(try exportText(store).contains("the thing that went wrong"))
  }

  func testNonErrorLevelsDoNotPin() throws {
    let store = makeStore(segmentBytes: 4096)
    store.install(root: root)
    let dir = try liveLaunch(store)

    for level in [LogStore.Level.trace, .debug, .info, .notice, .warn] {
      store.record(level: level, label: "t", message: "noise")
    }
    store.flush()

    XCTAssertFalse(FileManager.default.fileExists(atPath: dir.appendingPathComponent("0000.keep").path))
    XCTAssertEqual(store.archives().first?.pinned, false)
  }

  // MARK: - launch ring

  func testLaunchRingDropsTheOldestUnpinnedLaunches() throws {
    try seedLaunch(id: "1700000001000", segments: [(0, "oldest\n", false)])
    try seedLaunch(id: "1700000002000", segments: [(0, "middle\n", false)])
    try seedLaunch(id: "1700000003000", segments: [(0, "newest\n", false)])

    let store = makeStore(launches: 3)
    store.install(root: root)

    let ids = store.archives().map(\.id)
    XCTAssertEqual(ids.count, 3)
    XCTAssertFalse(ids.contains("1700000001000"))
    XCTAssertTrue(ids.contains("1700000002000"))
    XCTAssertTrue(ids.contains("1700000003000"))
  }

  func testLaunchRingIgnoresPinnedLaunches() throws {
    try seedLaunch(id: "1700000001000", segments: [(0, "pinned oldest\n", true)])
    try seedLaunch(id: "1700000002000", segments: [(0, "plain\n", false)])
    try seedLaunch(id: "1700000003000", segments: [(0, "plain\n", false)])
    try seedLaunch(id: "1700000004000", segments: [(0, "plain\n", false)])

    let store = makeStore(launches: 2)
    store.install(root: root)

    let ids = store.archives().map(\.id)
    XCTAssertTrue(ids.contains("1700000001000"))
    XCTAssertFalse(ids.contains("1700000002000"))
    XCTAssertFalse(ids.contains("1700000003000"))
    XCTAssertTrue(ids.contains("1700000004000"))
  }

  func testPinnedBytesLimitShedsTheOldestPinnedLaunch() throws {
    let body = String(repeating: "e", count: 600)
    try seedLaunch(id: "1700000001000", segments: [(0, body, true)])
    try seedLaunch(id: "1700000002000", segments: [(0, body, true)])
    try seedLaunch(id: "1700000003000", segments: [(0, body, true)])

    let store = makeStore(pinnedBytesLimit: 1500)
    store.install(root: root)

    let ids = store.archives().map(\.id)
    XCTAssertFalse(ids.contains("1700000001000"))
    XCTAssertTrue(ids.contains("1700000002000"))
    XCTAssertTrue(ids.contains("1700000003000"))
  }

  func testPinnedBytesLimitKeepsEverythingUnderTheCap() throws {
    let body = String(repeating: "e", count: 100)
    try seedLaunch(id: "1700000001000", segments: [(0, body, true)])
    try seedLaunch(id: "1700000002000", segments: [(0, body, true)])

    let store = makeStore(pinnedBytesLimit: 1500)
    store.install(root: root)

    let ids = store.archives().map(\.id)
    XCTAssertTrue(ids.contains("1700000001000"))
    XCTAssertTrue(ids.contains("1700000002000"))
  }

  // MARK: - archives

  func testArchivesAreNewestFirstAndFlagTheLiveLaunch() throws {
    try seedLaunch(id: "1700000001000", segments: [(0, "old\n", false)])
    try seedLaunch(id: "1700000002000", segments: [(0, "err\n", true)])

    let store = makeStore()
    store.install(root: root)
    store.record(level: .info, label: "t", message: "live")
    store.flush()

    let archives = store.archives()
    XCTAssertEqual(archives.map(\.startedAtMs), archives.map(\.startedAtMs).sorted(by: >))
    XCTAssertTrue(archives[0].current)
    XCTAssertEqual(archives.filter(\.current).count, 1)
    XCTAssertEqual(archives.first { $0.id == "1700000002000" }?.pinned, true)
    XCTAssertEqual(archives.first { $0.id == "1700000001000" }?.pinned, false)
    XCTAssertEqual(archives.first { $0.id == "1700000001000" }?.bytes, 4)
  }

  func testRetainedBytesSumsEveryLaunch() throws {
    try seedLaunch(id: "1700000001000", segments: [(0, "12345", false)])
    try seedLaunch(id: "1700000002000", segments: [(0, "123", false), (1, "12", false)])

    let store = makeStore()
    store.install(root: root)
    XCTAssertEqual(store.retainedBytes(), 10)
  }

  // MARK: - export

  func testExportHeaderAndBannersDescribeEveryLaunch() throws {
    try seedLaunch(id: "1700000001000", segments: [(0, "from an older run\n", false)])
    try seedLaunch(id: "1700000002000", segments: [(0, "from a run that failed\n", true)])

    let store = makeStore()
    store.install(root: root)
    store.record(level: .info, label: "t", message: "from the live run")
    store.flush()

    let text = try exportText(store)
    XCTAssertTrue(text.hasPrefix("bridgething log export\n"))
    XCTAssertTrue(text.contains("\nlaunches: 3\n"))
    XCTAssertFalse(text.contains("dropped lines"))
    XCTAssertTrue(text.contains("[pinned: contains errors]"))
    XCTAssertTrue(text.contains("(current)"))

    let older = try XCTUnwrap(text.range(of: "from an older run"))
    let failed = try XCTUnwrap(text.range(of: "from a run that failed"))
    let live = try XCTUnwrap(text.range(of: "from the live run"))
    XCTAssertLessThan(older.lowerBound, failed.lowerBound)
    XCTAssertLessThan(failed.lowerBound, live.lowerBound)
  }

  func testExportWithAnIdNarrowsToThatLaunch() throws {
    try seedLaunch(id: "1700000001000", segments: [(0, "from an older run\n", false)])
    try seedLaunch(id: "1700000002000", segments: [(0, "from a newer run\n", false)])

    let store = makeStore()
    store.install(root: root)

    let text = try exportText(store, id: "1700000001000")
    XCTAssertTrue(text.contains("\nlaunches: 1\n"))
    XCTAssertTrue(text.contains("from an older run"))
    XCTAssertFalse(text.contains("from a newer run"))
  }

  func testExportConcatenatesSegmentsInOrder() throws {
    try seedLaunch(id: "1700000001000", segments: [(0, "alpha\n", false), (1, "beta\n", false), (2, "gamma\n", false)])

    let store = makeStore()
    store.install(root: root)

    let text = try exportText(store, id: "1700000001000")
    let alpha = try XCTUnwrap(text.range(of: "alpha"))
    let beta = try XCTUnwrap(text.range(of: "beta"))
    let gamma = try XCTUnwrap(text.range(of: "gamma"))
    XCTAssertLessThan(alpha.lowerBound, beta.lowerBound)
    XCTAssertLessThan(beta.lowerBound, gamma.lowerBound)
  }

  func testExportTerminatesASegmentThatLacksATrailingNewline() throws {
    try seedLaunch(id: "1700000001000", segments: [(0, "unterminated", false), (1, "next\n", false)])

    let store = makeStore()
    store.install(root: root)

    XCTAssertTrue(try exportText(store, id: "1700000001000").contains("unterminated\nnext"))
  }

  func testExportOverwritesAnExistingBundle() throws {
    try seedLaunch(id: "1700000001000", segments: [(0, "short\n", false)])
    let store = makeStore()
    store.install(root: root)

    let target = root.appendingPathComponent("bundle.txt")
    try Data(String(repeating: "stale", count: 500).utf8).write(to: target)
    try store.exportTo(target, id: "1700000001000")

    let text = try String(contentsOf: target, encoding: .utf8)
    XCTAssertFalse(text.contains("stale"))
    XCTAssertTrue(text.contains("short"))
  }

  // MARK: - delete and clear

  func testDeleteDropsAPastLaunchAndLeavesTheRest() throws {
    try seedLaunch(id: "1700000001000", segments: [(0, "gone\n", false)])
    try seedLaunch(id: "1700000002000", segments: [(0, "kept\n", false)])

    let store = makeStore()
    store.install(root: root)
    store.delete(id: "1700000001000")

    XCTAssertFalse(store.archives().contains { $0.id == "1700000001000" })
    XCTAssertTrue(store.archives().contains { $0.id == "1700000002000" })
  }

  func testDeleteTruncatesTheLiveLaunchAndKeepsRecording() throws {
    let store = makeStore()
    store.install(root: root)
    store.record(level: .info, label: "t", message: "before")
    store.flush()

    let id = try XCTUnwrap(store.archives().first { $0.current }?.id)
    store.delete(id: id)
    XCTAssertEqual(store.archives().first { $0.id == id }?.bytes, 0)

    store.record(level: .info, label: "t", message: "after")
    store.flush()

    let text = try exportText(store)
    XCTAssertFalse(text.contains("before"))
    XCTAssertTrue(text.contains("after"))
  }

  func testDeleteIgnoresIdsThatAreNotLaunchDirectories() throws {
    try seedLaunch(id: "1700000001000", segments: [(0, "kept\n", false)])
    let store = makeStore()
    store.install(root: root)

    store.delete(id: "../1700000001000")
    store.delete(id: "not-a-launch")
    store.delete(id: "")

    XCTAssertTrue(store.archives().contains { $0.id == "1700000001000" })
  }

  func testClearRemovesPinnedLaunchesAndTruncatesTheLiveOne() throws {
    try seedLaunch(id: "1700000001000", segments: [(0, "plain\n", false)])
    try seedLaunch(id: "1700000002000", segments: [(0, "pinned\n", true)])

    let store = makeStore()
    store.install(root: root)
    store.record(level: .error, label: "t", message: "live error")
    store.flush()

    store.clear()

    let archives = store.archives()
    XCTAssertEqual(archives.count, 1)
    XCTAssertTrue(archives[0].current)
    XCTAssertEqual(archives[0].bytes, 0)
    XCTAssertFalse(archives[0].pinned)
    XCTAssertEqual(store.retainedBytes(), 0)
  }

  func testClearedLiveLaunchStillRotatesAfterwards() throws {
    let store = makeStore(segmentsPerLaunch: 2, segmentBytes: 256)
    store.install(root: root)
    let dir = try liveLaunch(store)

    store.record(level: .error, label: "t", message: "pin me")
    store.flush()
    store.clear()

    for i in 0 ..< 200 { store.record(level: .info, label: "t", message: "line \(i) \(String(repeating: "x", count: 60))") }
    store.flush()

    XCTAssertEqual(segmentNames(in: dir).count, 2)
    XCTAssertTrue(try exportText(store).contains("line 199"))
    XCTAssertFalse(try exportText(store).contains("pin me"))
  }

  // MARK: - install

  func testInstallIsIdempotent() throws {
    let store = makeStore()
    store.install(root: root)
    let first = try XCTUnwrap(store.archives().first?.id)
    store.install(root: root)
    XCTAssertEqual(store.archives().count, 1)
    XCTAssertEqual(store.archives().first?.id, first)
  }
}
