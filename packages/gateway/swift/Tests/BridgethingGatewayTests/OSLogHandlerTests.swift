import Logging
import XCTest
@testable import BridgethingGateway

final class OSLogHandlerTests: XCTestCase {
  func testRenderBareMessage() {
    XCTAssertEqual(OSLogHandler.render("hello", merged: [:]), "hello")
  }

  func testRenderAppendsSortedMetadata() {
    let merged: Logging.Logger.Metadata = ["reason": "iap2-hint", "source": "poll"]
    XCTAssertEqual(OSLogHandler.render("merge", merged: merged), "merge reason=iap2-hint source=poll")
  }
}
