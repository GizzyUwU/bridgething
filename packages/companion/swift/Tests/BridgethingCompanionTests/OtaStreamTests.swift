import BridgethingGateway
import BridgethingGlue
import BridgethingSchema
import BridgethingTestKit
import Foundation
import XCTest

@testable import BridgethingCompanion

final class OtaStreamTests: XCTestCase {
    private struct Harness {
        let companion: BridgethingCompanion
        let driver: WireDriver
    }

    private static let fragmentBytes = TransferPacer.largeFragmentBytes
    private static let windowBytes = Int(TransferPacer.maxWindowBytes)

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
        let payloadSize = 96 * 1024
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

        var assembled = Data()
        let first = try await nextFragment(h.driver, transferId)
        XCTAssertEqual(Int(first.offset), 0)
        XCTAssertLessThanOrEqual(first.bytes.count, Self.fragmentBytes, "ota fragments must stay within one frame")
        assembled.append(first.bytes)
        do {
            _ = try await nextFragment(h.driver, transferId, timeout: .milliseconds(600))
            XCTFail("sender ran past the initial one-fragment window without an ack")
        } catch is WireDriverError {
            // expected: window full, sender blocked on the ack.
        }

        var acked = UInt32(assembled.count)
        try await ack(h.driver, transferId, acked)

        while assembled.count < payloadSize {
            let f = try await nextFragment(h.driver, transferId)
            XCTAssertEqual(Int(f.offset), assembled.count, "fragments must arrive in offset order")
            XCTAssertLessThanOrEqual(f.bytes.count, Self.fragmentBytes)
            XCTAssertLessThan(Int(f.offset), Int(acked) + Self.windowBytes, "sender must stay within the max window of acked")
            assembled.append(f.bytes)
            acked = f.offset + UInt32(f.bytes.count)
            try await ack(h.driver, transferId, acked)
        }
        XCTAssertEqual(assembled.count, payloadSize)

        try await h.driver.send(.system(.otaProgress(OtaProgress(phase: .writing, percent: 100, step: 0, nsteps: 0, dwlPercent: 0, dwlBytes: 0, etaMs: nil))), meta: .event)
        _ = try await h.driver.waitOutbound(timeout: .seconds(3)) { m in
            if case .system(.otaActivate) = m.data { return true }
            return false
        }
        try await h.driver.send(.system(.otaProgress(OtaProgress(phase: .reboot, percent: 100, step: 0, nsteps: 0, dwlPercent: 0, dwlBytes: 0, etaMs: nil))), meta: .event)

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

        for _ in 0 ..< 2 {
            let f = try await nextFragment(h.driver, transferId)
            try await ack(h.driver, transferId, f.offset + UInt32(f.bytes.count))
        }

        try await h.driver.send(.system(.otaError(OtaError(code: .offsetMismatch, msg: "synthetic"))), meta: .event)

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

    private func writeFilledZck(_ bytes: Int, fill: UInt8) throws -> URL {
        let payload = Data(repeating: fill, count: bytes)
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("zck-\(UUID()).bin")
        try payload.write(to: url)
        return url
    }

    func testRangeRequestRoutesByAsset() async throws {
        let h = try await boot()
        let systemZck = try writeFilledZck(256, fill: 0xAA)
        let bootZck = try writeFilledZck(256, fill: 0xBB)
        defer {
            try? FileManager.default.removeItem(at: systemZck)
            try? FileManager.default.removeItem(at: bootZck)
        }
        await h.companion.ota.setLocalZcks([
            OtaService.systemZckAsset: systemZck,
            OtaService.bootZckAsset: bootZck,
        ])

        func requestRange(asset: String) async throws -> GatewayToBridgeMsg {
            try await h.driver.request(.system(.otaAssetRange(OtaAssetRange(
                updateId: "u1",
                asset: asset,
                ranges: [RangeSpec(start: 0, length: 256)]
            ))))
        }

        let bootReply = try await requestRange(asset: OtaService.bootZckAsset)
        guard case let .system(.otaAssetRangeReply(reply)) = bootReply.data, case let .inline(body) = reply.body else {
            return XCTFail("expected inline range reply for boot asset, got \(bootReply.data)")
        }
        XCTAssertEqual(Array(body), Array(repeating: UInt8(0xBB), count: 256), "boot asset must be served from the boot zck")

        let systemReply = try await requestRange(asset: OtaService.systemZckAsset)
        guard case let .system(.otaAssetRangeReply(sysReply)) = systemReply.data, case let .inline(sysBody) = sysReply.body else {
            return XCTFail("expected inline range reply for system asset, got \(systemReply.data)")
        }
        XCTAssertEqual(Array(sysBody), Array(repeating: UInt8(0xAA), count: 256), "system asset must be served from the system zck")

        let unknownReply = try await requestRange(asset: "does-not-exist.zck")
        guard case let .system(.otaAssetRangeRejected(rej)) = unknownReply.data else {
            return XCTFail("expected rejection for unknown asset, got \(unknownReply.data)")
        }
        XCTAssertTrue(rej.reason.contains("does-not-exist.zck"), "rejection must name the missing asset")

        await h.companion.stop()
    }

    func testRangeStreamWindowsAgainstAcks() async throws {
        let h = try await boot()
        let size = 256 * 1024
        let window = Int(TransferPacer.maxWindowBytes)
        let zck = try writeTempArtifact(size)
        defer { try? FileManager.default.removeItem(at: zck) }
        await h.companion.ota.setLocalZcks([OtaService.systemZckAsset: zck])

        let reply = try await h.driver.request(.system(.otaAssetRange(OtaAssetRange(
            updateId: "u1", asset: OtaService.systemZckAsset, ranges: [RangeSpec(start: 0, length: UInt32(size))]
        ))))
        guard case let .system(.otaAssetRangeReply(r)) = reply.data, case let .stream(ref) = r.body else {
            return XCTFail("expected a streamed range reply, got \(reply.data)")
        }
        let transferId = ref.id

        var assembled = Data()
        let first = try await nextFragment(h.driver, transferId)
        XCTAssertEqual(Int(first.offset), 0)
        assembled.append(first.bytes)
        do {
            _ = try await nextFragment(h.driver, transferId, timeout: .milliseconds(600))
            XCTFail("range sender ran past the initial one-fragment window without an ack")
        } catch is WireDriverError {
            // expected: window full, sender blocked on the ack.
        }

        var acked = UInt32(assembled.count)
        try await ack(h.driver, transferId, acked)
        while assembled.count < size {
            let f = try await nextFragment(h.driver, transferId)
            XCTAssertEqual(Int(f.offset), assembled.count, "range fragments must arrive contiguous in offset order")
            XCTAssertLessThan(Int(f.offset), Int(acked) + window, "range sender must stay within the max window of acked")
            assembled.append(f.bytes)
            acked = f.offset + UInt32(f.bytes.count)
            try await ack(h.driver, transferId, acked)
        }
        XCTAssertEqual(assembled.count, size)
        XCTAssertEqual(Array(assembled), (0 ..< size).map { UInt8($0 % 251) }, "streamed range bytes must match the zck")
        await h.companion.stop()
    }

    func testOtaResumeFromNonZeroOffsetStreamsRemainder() async throws {
        let h = try await boot()
        let payloadSize = 160 * 1024
        let resumeOffset: UInt32 = 64 * 1024
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

        let transferId = try await answerBegin(h.driver, resumeFromOffset: resumeOffset)

        let first = try await nextFragment(h.driver, transferId)
        XCTAssertEqual(first.offset, resumeOffset, "first fragment must resume at the daemon's offset, not 0")
        var expected = first.offset + UInt32(first.bytes.count)
        try await ack(h.driver, transferId, expected)

        while Int(expected) < payloadSize {
            let f = try await nextFragment(h.driver, transferId)
            XCTAssertEqual(f.offset, expected, "resume fragments must arrive contiguous in offset order")
            XCTAssertLessThanOrEqual(f.bytes.count, Self.fragmentBytes)
            expected = f.offset + UInt32(f.bytes.count)
            try await ack(h.driver, transferId, expected)
        }
        XCTAssertEqual(Int(expected), payloadSize, "the whole remainder past resumeOffset must stream")

        try await h.driver.send(.system(.otaProgress(OtaProgress(phase: .writing, percent: 100, step: 0, nsteps: 0, dwlPercent: 0, dwlBytes: 0, etaMs: nil))), meta: .event)
        _ = try await h.driver.waitOutbound(timeout: .seconds(3)) { m in
            if case .system(.otaActivate) = m.data { return true }
            return false
        }
        try await h.driver.send(.system(.otaProgress(OtaProgress(phase: .reboot, percent: 100, step: 0, nsteps: 0, dwlPercent: 0, dwlBytes: 0, etaMs: nil))), meta: .event)

        var terminal: OtaPhaseSnapshot?
        for await snap in progress { terminal = snap }
        await pushTask.value
        guard case .completed = terminal else {
            return XCTFail("expected completed terminal, got \(String(describing: terminal))")
        }
        await h.companion.stop()
    }

    // MARK: - apply-version precedence (image-change subsumes the daemon bandaid)

    private func otaCacheDir() -> URL {
        let base = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first
            ?? URL(fileURLWithPath: NSTemporaryDirectory())
        return base.appendingPathComponent("bridgething-ota", isDirectory: true)
    }

    @discardableResult
    private func seedArtifact(_ dir: URL, _ name: String, bytes: Int) throws -> URL {
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let url = dir.appendingPathComponent(name)
        try Data((0 ..< bytes).map { UInt8($0 % 251) }).write(to: url)
        return url
    }

    private func makeMeta(appVersion: String, imageVersion: String, channel: String, variant: String = "prod") -> BridgeThingMeta {
        BridgeThingMeta(
            bridgethingVersion: appVersion, libbridgethingVersion: appVersion, appName: "bridgething",
            nickname: nil, appVersion: appVersion, osName: "linux", osVersion: "1", osDescription: "",
            btMac: "", serialNumber: "", fccId: "", icId: "", modelName: "Car Thing", channel: channel,
            imageVariant: variant, imageVersion: imageVersion, imageBuildId: "", imageBuildDate: "",
            imageDistro: "", imageMachine: "", discord: "", credits: ""
        )
    }

    private func injectMeta(_ h: Harness, _ meta: BridgeThingMeta) async throws {
        try await h.driver.send(.version(meta), meta: .event)
        for _ in 0 ..< 100 {
            if await h.companion.ota.meta(deviceId: h.driver.deviceId) != nil { return }
            try await Task.sleep(for: .milliseconds(20))
        }
        XCTFail("device meta was never recorded by the ota service")
    }

    private func nextOtaBegin(_ driver: WireDriver, timeout: Duration = .seconds(3)) async throws -> (id: UUID, begin: OtaBegin) {
        let msg = try await driver.waitOutbound(timeout: timeout) { m in
            if case .system(.otaBegin) = m.data { return true }
            return false
        }
        guard case let .system(.otaBegin(begin)) = msg.data else { throw WireDriverError.decodeFailed }
        return (msg.id, begin)
    }

    func testApplyVersionImageChangeRunsImageOnly() async throws {
        let h = try await boot()
        let channel = "stable"
        let dir = otaCacheDir()
        let swu = try seedArtifact(dir, "image-\(channel)-2026.05.0.swu", bytes: 2048)
        let zck = try seedArtifact(dir, "image-\(channel)-2026.05.0.zck", bytes: 256)
        let bootZck = try seedArtifact(dir, "image-\(channel)-2026.05.0-boot.zck", bytes: 256)
        let daemon = try seedArtifact(dir, "daemon-\(channel)-0.8.4", bytes: 512)
        defer { for u in [swu, zck, bootZck, daemon] { try? FileManager.default.removeItem(at: u) } }

        try await injectMeta(h, makeMeta(appVersion: "0.8.3", imageVersion: "2026.04.0", channel: channel))

        let applyTask = Task {
            await h.companion.ota.applyVersion(
                deviceId: h.driver.deviceId, channel: channel,
                version: "0.8.4+image.2026.05.0", rootURL: URL(string: "https://ota.invalid")!
            )
        }

        let (beginId, begin) = try await nextOtaBegin(h.driver)
        XCTAssertEqual(begin.kind, .image, "an image change must run the image OTA, not the daemon bandaid")

        try await h.driver.send(
            .system(.otaBeginAck(OtaBeginAck(resumeFromOffset: 0))),
            meta: .response(ResponseMeta(requestId: beginId))
        )
        try await drainFragments(h.driver, begin.transfer.id, total: Int(begin.transfer.totalSize))
        try await h.driver.send(.system(.otaProgress(OtaProgress(phase: .reboot, percent: 100, step: 0, nsteps: 0, dwlPercent: 0, dwlBytes: 0, etaMs: nil))), meta: .event)
        await applyTask.value

        for frame in await h.driver.outboundFrames() {
            if case let .system(.otaBegin(b)) = frame.data {
                XCTAssertNotEqual(b.kind, .daemon, "no standalone daemon bandaid push while the image is changing")
            }
        }
        await h.companion.stop()
    }

    func testApplyVersionDaemonOnlyRunsBandaid() async throws {
        let h = try await boot()
        let channel = "stable"
        let dir = otaCacheDir()
        let daemon = try seedArtifact(dir, "daemon-\(channel)-0.8.4", bytes: 512)
        defer { try? FileManager.default.removeItem(at: daemon) }

        try await injectMeta(h, makeMeta(appVersion: "0.8.3", imageVersion: "2026.05.0", channel: channel))

        let applyTask = Task {
            await h.companion.ota.applyVersion(
                deviceId: h.driver.deviceId, channel: channel,
                version: "0.8.4+image.2026.05.0", rootURL: URL(string: "https://ota.invalid")!
            )
        }

        let (beginId, begin) = try await nextOtaBegin(h.driver)
        XCTAssertEqual(begin.kind, .daemon, "a daemon-only delta must run the daemon bandaid")

        try await h.driver.send(
            .system(.otaBeginAck(OtaBeginAck(resumeFromOffset: 0))),
            meta: .response(ResponseMeta(requestId: beginId))
        )
        try await drainFragments(h.driver, begin.transfer.id, total: Int(begin.transfer.totalSize))
        try await h.driver.send(.system(.otaProgress(OtaProgress(phase: .writing, percent: 100, step: 0, nsteps: 0, dwlPercent: 0, dwlBytes: 0, etaMs: nil))), meta: .event)
        _ = try await h.driver.waitOutbound(timeout: .seconds(3)) { m in
            if case .system(.otaActivate) = m.data { return true }
            return false
        }
        try await h.driver.send(.system(.otaProgress(OtaProgress(phase: .reboot, percent: 100, step: 0, nsteps: 0, dwlPercent: 0, dwlBytes: 0, etaMs: nil))), meta: .event)
        await applyTask.value

        for frame in await h.driver.outboundFrames() {
            if case let .system(.otaBegin(b)) = frame.data {
                XCTAssertNotEqual(b.kind, .image, "a daemon-only delta must not start an image OTA")
            }
        }
        await h.companion.stop()
    }

    private func drainFragments(_ driver: WireDriver, _ transferId: UUID, total: Int) async throws {
        var sent = 0
        while sent < total {
            let f = try await nextFragment(driver, transferId)
            sent = Int(f.offset) + f.bytes.count
            try await ack(driver, transferId, UInt32(sent))
        }
    }
}
