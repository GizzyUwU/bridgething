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

    private func simulate(linkBytesPerSec: Double, rtt: Double, seconds: Double) -> (throughput: Double, window: UInt64) {
        var (pacer, clock) = makePacer()
        var acked: UInt64 = 0
        var elapsed = 0.0
        while elapsed < seconds {
            let batch = pacer.windowBytes
            let onWire = Double(batch) / linkBytesPerSec
            let step = max(onWire, rtt)
            clock.advance(step)
            elapsed += step
            acked += batch
            pacer.observe(ackedBytes: acked)
        }
        return (Double(acked) / elapsed, pacer.windowBytes)
    }

    func testFloorSpansSeveralAckIntervalsSoTheStreamNeverStopsAndWaits() {
        let (pacer, _) = makePacer()
        XCTAssertGreaterThanOrEqual(pacer.windowBytes, 4 * TransferPacer.ackIntervalBytes)
        XCTAssertGreaterThanOrEqual(
            pacer.windowBytes / UInt64(pacer.fragmentBytes), 4,
            "at least four fragments must be in flight before the first ack is needed"
        )
    }

    func testReachesLinkRateOverBluetooth() {
        let link = 175_000.0
        let (throughput, _) = simulate(linkBytesPerSec: link, rtt: 0.25, seconds: 60)
        XCTAssertGreaterThan(
            throughput, link * 0.9,
            "pacer must not be the constraint on a link this slow; got \(Int(throughput)) B/s of \(Int(link))"
        )
    }

    func testReachesLinkRateWhenTheRoundTripIsLong() {
        let link = 175_000.0
        let (throughput, _) = simulate(linkBytesPerSec: link, rtt: 0.5, seconds: 120)
        XCTAssertGreaterThan(throughput, link * 0.9, "got \(Int(throughput)) B/s of \(Int(link))")
    }

    func testWindowStaysInsideTheQueueingBudget() {
        let link = 175_000.0
        let (_, window) = simulate(linkBytesPerSec: link, rtt: 0.25, seconds: 60)
        let queued = Double(window) / link
        XCTAssertLessThanOrEqual(queued, TransferPacer.targetDelaySeconds * 1.5, "queued \(queued)s of link time")
    }

    func testWindowStaysInsideTheDaemonsBufferedDepth() {
        let (_, window) = simulate(linkBytesPerSec: 20_000_000, rtt: 0.002, seconds: 5)
        XCTAssertLessThanOrEqual(window, TransferPacer.maxWindowBytes)
        XCTAssertLessThanOrEqual(window / UInt64(TransferPacer.fragmentBytes), 16)
    }

    func testATransientStallDoesNotCollapseTheWindow() {
        var (pacer, clock) = makePacer()
        var acked: UInt64 = 0
        for _ in 0 ..< 8 {
            clock.advance(0.25)
            acked += 44 * 1024
            pacer.observe(ackedBytes: acked)
        }
        let settled = pacer.windowBytes
        XCTAssertGreaterThan(settled, TransferPacer.minWindowBytes)

        clock.advance(4.0)
        acked += 4 * 1024
        pacer.observe(ackedBytes: acked)
        XCTAssertEqual(pacer.windowBytes, settled, "one slow sample must not shed the window")
    }

    func testSustainedDegradationDoesShrinkTheWindow() {
        var (pacer, clock) = makePacer()
        var acked: UInt64 = 0
        for _ in 0 ..< 8 {
            clock.advance(0.25)
            acked += 128 * 1024
            pacer.observe(ackedBytes: acked)
        }
        let fast = pacer.windowBytes
        for _ in 0 ..< TransferPacer.rateSampleCount {
            clock.advance(2.0)
            acked += 8 * 1024
            pacer.observe(ackedBytes: acked)
        }
        XCTAssertLessThan(pacer.windowBytes, fast, "a link that is genuinely slow now must queue less")
        XCTAssertGreaterThanOrEqual(pacer.windowBytes, TransferPacer.minWindowBytes)
    }

    func testNonAdvancingAckIsIgnored() {
        var (pacer, clock) = makePacer(startOffset: 8 * 1024)
        clock.advance(5.0)
        pacer.observe(ackedBytes: 8 * 1024)
        pacer.observe(ackedBytes: 4 * 1024)
        XCTAssertNil(pacer.ratePerSec, "no rate estimate without progress")
        XCTAssertEqual(pacer.windowBytes, TransferPacer.minWindowBytes)
    }

    func testResumeBaselineDoesNotInventAHugeFirstSample() {
        var (pacer, clock) = makePacer(startOffset: 30 * 1024 * 1024)
        clock.advance(0.25)
        pacer.observe(ackedBytes: 30 * 1024 * 1024 + 44 * 1024)
        let rate = pacer.ratePerSec ?? 0
        XCTAssertLessThan(rate, 1_000_000, "rate came out as \(Int(rate)) B/s, which means the baseline was 0")
    }
}
