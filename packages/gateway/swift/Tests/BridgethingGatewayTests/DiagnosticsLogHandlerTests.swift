import Logging
import XCTest
@testable import BridgethingGateway

final class DiagnosticsLogHandlerTests: XCTestCase {
  func testLogTeesIntoDiagnosticsBuffer() {
    let handler = DiagnosticsLogHandler(label: "test.facade")
    let marker = "facade-marker-\(UUID().uuidString)"

    handler.log(
      level: .warning, message: "\(marker)", metadata: nil,
      source: "", file: #file, function: #function, line: #line
    )

    let record = DiagnosticsBuffer.shared.tail(limit: 256).last { $0.message == marker }
    XCTAssertNotNil(record)
    XCTAssertEqual(record?.kind, .log)
    XCTAssertEqual(record?.level, "warning")
    XCTAssertEqual(record?.target, "test.facade")
  }

  func testLogAppendsMetadata() {
    let handler = DiagnosticsLogHandler(label: "test.facade")
    let marker = "meta-marker-\(UUID().uuidString)"

    handler.log(
      level: .info, message: "\(marker)", metadata: ["reason": "iap2-hint"],
      source: "", file: #file, function: #function, line: #line
    )

    let record = DiagnosticsBuffer.shared.tail(limit: 256).last { $0.message?.hasPrefix(marker) == true }
    XCTAssertNotNil(record)
    XCTAssertEqual(record?.message, "\(marker) reason=iap2-hint")
  }
}
