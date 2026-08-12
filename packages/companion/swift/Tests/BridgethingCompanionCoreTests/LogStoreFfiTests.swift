import BridgethingCompanionCore
import Foundation
import XCTest

final class LogStoreFfiTests: XCTestCase {
  private var root: URL!

  override func setUpWithError() throws {
    root = FileManager.default.temporaryDirectory
      .appendingPathComponent("logstore-ffi-\(UUID().uuidString)", isDirectory: true)
  }

  override func tearDownWithError() throws {
    try? FileManager.default.removeItem(at: root)
  }

  func testARecordPersistsThroughTheFfiAndComesBackInAnExport() throws {
    let store = LogStore.install(root: root.path)
    store.record(level: .warn, label: "daemon", message: "[player] stalled")
    store.flush()

    let bundle = root.appendingPathComponent("bundle.txt")
    XCTAssertEqual(try store.exportTo(target: bundle.path, id: nil), bundle.path)
    let text = try String(contentsOf: bundle, encoding: .utf8)
    XCTAssertTrue(text.hasPrefix("bridgething log export\n"))
    XCTAssertTrue(text.contains(" W daemon: [player] stalled"))
  }

  func testARawLineAtErrorSeverityPinsItsLaunch() throws {
    let store = LogStore.install(root: root.path)
    store.write(line: "07-30 12:00:00.000  1  1 E BridgethingBT: rfcomm connect failed")
    store.flush()

    let live = try XCTUnwrap(store.archives().first { $0.current })
    XCTAssertTrue(live.pinned)
    XCTAssertGreaterThan(live.bytes, 0)
  }

  func testClearEmptiesTheLiveLaunchAndLeavesItRecordable() throws {
    let store = LogStore.install(root: root.path)
    store.record(level: .info, label: "daemon", message: "before")
    store.flush()
    store.clear()

    XCTAssertEqual(store.retainedBytes(), 0)
    XCTAssertEqual(store.archives().first { $0.current }?.pinned, false)

    store.record(level: .info, label: "daemon", message: "after")
    store.flush()
    XCTAssertGreaterThan(store.retainedBytes(), 0)
  }
}
