import BridgethingCompanionCore
import BridgethingCompanion
import Foundation
import XCTest

final class FoundationHostEnvironmentTests: XCTestCase {
    func testClockReportsARealWallClockAndSplitOffsets() {
        let clock = FoundationHostEnvironment().clock()

        XCTAssertFalse(clock.tzIana.isEmpty)
        XCTAssertGreaterThan(clock.unixSeconds, 1_700_000_000)

        let now = Date()
        let tz = TimeZone.current
        let total = tz.secondsFromGMT(for: now) / 60
        XCTAssertEqual(Int(clock.utcOffsetMinutes) + Int(clock.dstOffsetMinutes), total)
    }
}
