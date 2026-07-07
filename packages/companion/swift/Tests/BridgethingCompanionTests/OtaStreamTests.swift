import BridgethingGateway
import BridgethingGlue
import BridgethingSchema
import BridgethingTestKit
import Foundation
import XCTest

@testable import BridgethingCompanion

/// Regression coverage for the OTA push pump: fragments are small and stay within one ack window
/// (the slow-link responsiveness + stale-fragment-collision fix), and a daemon error mid-stream
/// cancels the stream and abandons the transfer instead of draining bytes into the next attempt.
final class OtaStreamTests: XCTestCase {
    private struct Harness {
        let companion: BridgethingCompanion
        let driver: WireDriver
    }

    private static let fragmentBytes = 4 * 1024
    private static let windowBytes = 32 * 1024

    private func boot() async throws -> Harness {
        let adapter = InMemoryAdapter()
        let companion = BridgethingCompanion(
            adapter: adapter,
            lyricsResolver: FakeLyricsResolver(),
            host: HostInfo(appName: "test-companion", appVersion: "0.0.1", osName: "macOS")
        )
        try await companion.setActive(FakeGlue())
        try await companion.start()
        let driver = WireDriver(adapter: adapter)
        await driver.start()
        driver.connect()
        return Harness(companion: companion, driver: driver)
    }

    private func writeTempArtifact(_ bytes: Int) throws -> URL {
        let payload = Data((0 ..< bytes).map { UInt8($0 % 251) })
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("ota-\(UUID()).bin")
        try payload.write(to: url)
        return url
    }

    /// Answer the companion's `OtaBegin` request with a resume offset; returns the per-attempt transfer id.
    private func answerBegin(_ driver: WireDriver, resumeFromOffset: UInt32 = 0) async throws -> UUID {
        let msg = try await driver.waitOutbound(timeout: .seconds(3)) { m in
            if case .system(.otaBegin) = m.data { return true }
            return false
        }
        guard case let .system(.otaBegin(begin)) = msg.data else { throw WireDriverError.decodeFailed }
        try await driver.send(
            .system(.otaBeginAck(OtaBeginAck(resumeFromOffset: resumeFromOffset))),
            meta: .response(ResponseMeta(requestId: msg.id))
        )
        return begin.transfer.id
    }

    private func nextFragment(
        _ driver: WireDriver,
        _ transferId: UUID,
        timeout: Duration = .seconds(3)
    ) async throws -> TransferFragment {
        let frame = try await driver.waitOutbound(timeout: timeout) { m in
            if case let .transfer(.fragment(f)) = m.data, f.transferId == transferId { return true }
            return false
        }
        guard case let .transfer(.fragment(f)) = frame.data else { throw WireDriverError.decodeFailed }
        return f
    }

    private func ack(_ driver: WireDriver, _ transferId: UUID, _ received: UInt32) async throws {
        try await driver.send(.transfer(.ack(TransferAck(transferId: transferId, received: received))), meta: .event)
    }

    func testOtaPushWindowsAndUsesSmallFragments() async throws {
        let h = try await boot()
        let payloadSize = 40 * 1024
        let artifact = try writeTempArtifact(payloadSize)
        defer { try? FileManager.default.removeItem(at: artifact) }

        let (progress, progressCont) = AsyncStream.makeStream(of: OtaPhaseSnapshot.self)
        let pushTask = Task {
            await h.companion.ota.pushDaemon(
                gateway: h.companion.gateway,
                deviceId: h.driver.deviceId,
                binaryPath: artifact,
                progress: progressCont
            )
        }

        let transferId = try await answerBegin(h.driver)

        // phase A: without an ack, the sender fills exactly one window then stalls. offset >= window is
        // NOT < acked(0) + window, so the fragment at the window boundary must not arrive.
        var assembled = Data()
        let inWindow = Self.windowBytes / Self.fragmentBytes
        for i in 0 ..< inWindow {
            let f = try await nextFragment(h.driver, transferId)
            XCTAssertEqual(Int(f.offset), i * Self.fragmentBytes, "fragments must arrive in offset order")
            XCTAssertLessThanOrEqual(f.bytes.count, Self.fragmentBytes, "ota fragments must be <= 4KB (small frames)")
            assembled.append(f.bytes)
        }
        do {
            _ = try await nextFragment(h.driver, transferId, timeout: .milliseconds(600))
            XCTFail("sender ran past the ack window without an ack")
        } catch is WireDriverError {
            // expected: window full, sender blocked on the ack.
        }

        // phase B: acking unblocks; the stream runs to completion staying within one window of acked.
        var acked = UInt32(assembled.count)
        try await ack(h.driver, transferId, acked)

        while assembled.count < payloadSize {
            let f = try await nextFragment(h.driver, transferId)
            XCTAssertEqual(Int(f.offset), assembled.count, "fragments must arrive in offset order")
            XCTAssertLessThanOrEqual(f.bytes.count, Self.fragmentBytes)
            XCTAssertLessThan(Int(f.offset), Int(acked) + Self.windowBytes, "sender must stay within one window of acked")
            assembled.append(f.bytes)
            acked = f.offset + UInt32(f.bytes.count)
            try await ack(h.driver, transferId, acked)
        }
        XCTAssertEqual(assembled.count, payloadSize)

        // drive the stage -> activate -> reboot terminal so pushDaemon returns cleanly.
        try await h.driver.send(.system(.otaProgress(OtaProgress(phase: .writing, percent: 100, etaMs: nil))), meta: .event)
        _ = try await h.driver.waitOutbound(timeout: .seconds(3)) { m in
            if case .system(.otaActivate) = m.data { return true }
            return false
        }
        try await h.driver.send(.system(.otaProgress(OtaProgress(phase: .reboot, percent: 100, etaMs: nil))), meta: .event)

        var terminal: OtaPhaseSnapshot?
        for await snap in progress { terminal = snap }
        await pushTask.value
        guard case .completed = terminal else {
            return XCTFail("expected completed terminal, got \(String(describing: terminal))")
        }
        await h.companion.stop()
    }

    func testOtaErrorCancelsStreamAndAbandons() async throws {
        let h = try await boot()
        let artifact = try writeTempArtifact(40 * 1024)
        defer { try? FileManager.default.removeItem(at: artifact) }

        let (progress, progressCont) = AsyncStream.makeStream(of: OtaPhaseSnapshot.self)
        let pushTask = Task {
            await h.companion.ota.pushDaemon(
                gateway: h.companion.gateway,
                deviceId: h.driver.deviceId,
                binaryPath: artifact,
                progress: progressCont
            )
        }

        let transferId = try await answerBegin(h.driver)

        // let a couple fragments flow (acked, so the stream is mid-flight rather than window-blocked).
        for _ in 0 ..< 2 {
            let f = try await nextFragment(h.driver, transferId)
            try await ack(h.driver, transferId, f.offset + UInt32(f.bytes.count))
        }

        // daemon reports a fatal error mid-stream.
        try await h.driver.send(.system(.otaError(OtaError(code: .offsetMismatch, msg: "synthetic"))), meta: .event)

        // the companion must abandon the transfer: cancel the stream + unbind the daemon-side sink so
        // no stale fragment can survive into the next attempt.
        let abandon = try await h.driver.waitOutbound(timeout: .seconds(3)) { m in
            if case let .transfer(.abandon(a)) = m.data, a.transferId == transferId { return true }
            return false
        }
        guard case let .transfer(.abandon(a)) = abandon.data else {
            return XCTFail("expected transfer.abandon")
        }
        XCTAssertEqual(a.transferId, transferId)

        var terminal: OtaPhaseSnapshot?
        for await snap in progress { terminal = snap }
        await pushTask.value
        guard case .failed = terminal else {
            return XCTFail("expected failed terminal, got \(String(describing: terminal))")
        }
        await h.companion.stop()
    }
}
