import BridgethingCompanionCore
import Logging
import XCTest
@testable import BridgethingCompanion

final class OSLogHandlerTests: XCTestCase {
  func testRenderBareMessage() {
    XCTAssertEqual(OSLogHandler.render("hello", merged: [:]), "hello")
  }

  func testRenderAppendsSortedMetadata() {
    let merged: Logging.Logger.Metadata = ["reason": "iap2-hint", "source": "poll"]
    XCTAssertEqual(OSLogHandler.render("merge", merged: merged), "merge reason=iap2-hint source=poll")
  }

  func testEveryLevelKeepsItsOwnStoreSeverity() {
    let mapped: [(Logging.Logger.Level, LogStoreLevel)] = [
      (.trace, .trace),
      (.debug, .debug),
      (.info, .info),
      (.notice, .notice),
      (.warning, .warn),
      (.error, .error),
      (.critical, .fatal),
    ]
    for (level, expected) in mapped {
      XCTAssertEqual(OSLogHandler.storeLevel(level), expected, "\(level)")
    }
  }

  func testHandlerPersistsRecordsAtOrBelowInfo() throws {
    let root = FileManager.default.temporaryDirectory
      .appendingPathComponent("oslog-\(UUID().uuidString)", isDirectory: true)
    defer { try? FileManager.default.removeItem(at: root) }

    let store = LogStore.install(root: root.path)

    let handler = OSLogHandler(label: "handler-test", store: store)
    handler.log(level: .debug, message: "a debug record", metadata: nil, source: "", file: "", function: "", line: 0)
    handler.log(level: .info, message: "an info record", metadata: ["k": "v"], source: "", file: "", function: "", line: 0)
    store.flush()

    let target = root.appendingPathComponent("bundle.txt")
    _ = try store.exportTo(target: target.path, id: nil)
    let text = try String(contentsOf: target, encoding: .utf8)
    XCTAssertTrue(text.contains(" D handler-test: a debug record"))
    XCTAssertTrue(text.contains(" I handler-test: an info record k=v"))
  }
}
