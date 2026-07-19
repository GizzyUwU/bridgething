import XCTest

@testable import BridgethingCompanion

final class TransferPacerTests: XCTestCase {
    private final class FakeClock {
        var now: Double = 0
        func advance(_ seconds: Double) { now += seconds }
    }

    private func makePacer(startOffset: UInt64 = 0) -> (TransferPacer, FakeClock) {
        let clock = FakeClock()
        let pacer = TransferPacer(startOffset: startOffset) { clock.now }
        return (pacer, clock)
    }

    func testInitialWindowIsOneLargeFragment() {
        let (pacer, _) = makePacer()
        XCTAssertEqual(pacer.windowBytes, UInt64(TransferPacer.largeFragmentBytes))
        XCTAssertEqual(pacer.fragmentBytes, TransferPacer.largeFragmentBytes)
    }

    func testFastAcksRideTheCapWithLargeFragments() {
        var (pacer, clock) = makePacer()
        var acked: UInt64 = 0
        for _ in 0 ..< 8 {
            clock.advance(0.1)
            acked += 16 * 1024 // 160 KB/s
            pacer.observe(ackedBytes: acked)
        }
        XCTAssertEqual(pacer.windowBytes, TransferPacer.maxWindowBytes)
        XCTAssertEqual(pacer.fragmentBytes, TransferPacer.largeFragmentBytes)
    }

    func testSlowAcksBoundQueueToTheTargetDelayWithSmallFragments() {
        var (pacer, clock) = makePacer()
        var acked: UInt64 = 0
        for _ in 0 ..< 10 {
            clock.advance(2.0)
            acked += 16 * 1024 // 8 KB/s, a wedged-link rate
            pacer.observe(ackedBytes: acked)
        }
        XCTAssertLessThanOrEqual(pacer.windowBytes, 8 * 1024)
        XCTAssertGreaterThanOrEqual(pacer.windowBytes, TransferPacer.minWindowBytes)
        XCTAssertEqual(pacer.fragmentBytes, TransferPacer.smallFragmentBytes)
    }

    func testDegradingLinkShedsQueueBeforeTheFloor() {
        var (pacer, clock) = makePacer()
        var acked: UInt64 = 0
        for _ in 0 ..< 8 {
            clock.advance(0.1)
            acked += 16 * 1024
            pacer.observe(ackedBytes: acked)
        }
        XCTAssertEqual(pacer.windowBytes, TransferPacer.maxWindowBytes)
        clock.advance(3.0)
        acked += 4 * 1024
        pacer.observe(ackedBytes: acked)
        XCTAssertLessThan(pacer.windowBytes, TransferPacer.maxWindowBytes)
    }

    func testRecoveryGrowsTheWindowBack() {
        var (pacer, clock) = makePacer()
        var acked: UInt64 = 0
        for _ in 0 ..< 10 {
            clock.advance(2.0)
            acked += 4 * 1024
            pacer.observe(ackedBytes: acked)
        }
        XCTAssertEqual(pacer.windowBytes, TransferPacer.minWindowBytes)
        for _ in 0 ..< 20 {
            clock.advance(0.05)
            acked += 16 * 1024
            pacer.observe(ackedBytes: acked)
        }
        XCTAssertEqual(pacer.windowBytes, TransferPacer.maxWindowBytes)
        XCTAssertEqual(pacer.fragmentBytes, TransferPacer.largeFragmentBytes)
    }

    func testNonAdvancingAckIsIgnored() {
        var (pacer, clock) = makePacer(startOffset: 8 * 1024)
        clock.advance(5.0)
        pacer.observe(ackedBytes: 8 * 1024)
        pacer.observe(ackedBytes: 4 * 1024)
        XCTAssertEqual(pacer.windowBytes, UInt64(TransferPacer.largeFragmentBytes), "no rate estimate without progress")
    }
}
