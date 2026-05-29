import BridgethingGateway
import BridgethingSchema
import BridgethingTestKit
import XCTest

@testable import BridgethingCompanion

final class TimeDispatchTests: XCTestCase {
    func testEmitsTimeSnapshotOnConnect() async throws {
        let adapter = InMemoryAdapter()
        let companion = BridgethingCompanion(
            adapter: adapter,
            lyricsResolver: FakeLyricsResolver(),
            host: HostInfo(appName: "time-test", appVersion: "0.0.1", osName: "macOS")
        )
        try await companion.start()
        let driver = WireDriver(adapter: adapter)
        await driver.start()
        driver.connect()

        let frame = try await driver.waitOutbound(timeout: .seconds(3)) { msg in
            if case .time(.snapshot) = msg.data { return true }
            return false
        }
        guard case let .time(.snapshot(info)) = frame.data else {
            await companion.stop()
            return XCTFail("expected time snapshot, got \(frame.data)")
        }
        XCTAssertNotNil(info.tzIana)
        XCTAssertNotNil(info.locale)
        XCTAssertGreaterThan(info.wallClockUnixS ?? 0, 1_700_000_000) // after 2023-11
        await companion.stop()
    }
}
