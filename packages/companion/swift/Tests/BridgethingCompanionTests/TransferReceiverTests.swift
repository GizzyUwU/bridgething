import BridgethingGateway
import BridgethingSchema
import BridgethingTestKit
import Foundation
import XCTest
#if canImport(CryptoKit)
    import CryptoKit
#endif

@testable import BridgethingCompanion

final class TransferReceiverTests: XCTestCase {
    private struct Harness {
        let gateway: BridgethingGateway
        let driver: WireDriver
        let receiver: TransferReceiver
    }

    private func boot() async throws -> Harness {
        let adapter = InMemoryAdapter()
        let gateway = BridgethingGateway(adapter: adapter)
        try await gateway.start()
        let driver = WireDriver(adapter: adapter)
        await driver.start()
        driver.connect()
        let receiver = TransferReceiver()
        await receiver.start(gateway: gateway)
        return Harness(gateway: gateway, driver: driver, receiver: receiver)
    }

    private func teardown(_ h: Harness) async {
        await h.receiver.stop()
        await h.driver.stop()
        await h.gateway.stop()
    }

    private func sha256hex(_ data: Data) -> String {
        #if canImport(CryptoKit)
            return SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        #else
            return ""
        #endif
    }

    private func stream(_ driver: WireDriver, _ transferId: UUID, _ payload: Data, fragmentBytes: Int) async throws {
        var offset = 0
        while offset < payload.count {
            let end = min(offset + fragmentBytes, payload.count)
            try await driver.send(
                .transfer(.fragment(TransferFragment(
                    transferId: transferId, offset: UInt32(offset), bytes: payload.subdata(in: offset ..< end)
                ))),
                meta: .event
            )
            offset = end
        }
    }

    private func nextAck(_ driver: WireDriver) async throws -> UInt32 {
        let frame = try await driver.waitOutbound(timeout: .seconds(3)) { m in
            if case .transfer(.ack) = m.data { return true }
            return false
        }
        guard case let .transfer(.ack(a)) = frame.data else { throw WireDriverError.decodeFailed }
        return a.received
    }

    func testStreamReassemblesAndCoalescesAcks() async throws {
        let h = try await boot()
        let payload = Data((0 ..< 40 * 1024).map { UInt8($0 % 251) })
        let transferId = UUID()
        let ref = TransferRef(id: transferId, totalSize: UInt32(payload.count), sha256: sha256hex(payload))
        await h.receiver.register(deviceId: h.driver.deviceId, ref: ref)

        let collectTask = Task { try await h.receiver.collect(transferId, timeout: .seconds(5)) }
        try await stream(h.driver, transferId, payload, fragmentBytes: 4096)

        let got = try await collectTask.value
        XCTAssertEqual(got, payload, "reassembled bytes must match the streamed payload")

        // 4 KiB fragments coalesce to one ack per 16 KiB crossed plus one always for the final byte.
        var acks: [UInt32] = []
        for _ in 0 ..< 3 { acks.append(try await nextAck(h.driver)) }
        XCTAssertEqual(acks, [16384, 32768, 40960], "acks must land on 16 KiB boundaries then the final byte")

        await teardown(h)
    }

    func testGapFailsCollect() async throws {
        let h = try await boot()
        let transferId = UUID()
        let ref = TransferRef(id: transferId, totalSize: 40 * 1024, sha256: nil)
        await h.receiver.register(deviceId: h.driver.deviceId, ref: ref)

        let collectTask = Task { try await h.receiver.collect(transferId, timeout: .seconds(5)) }
        try await h.driver.send(
            .transfer(.fragment(TransferFragment(transferId: transferId, offset: 0, bytes: Data(count: 4096)))),
            meta: .event
        )
        // skip offset 4096; a non-contiguous fragment must fail the transfer.
        try await h.driver.send(
            .transfer(.fragment(TransferFragment(transferId: transferId, offset: 8192, bytes: Data(count: 4096)))),
            meta: .event
        )
        do {
            _ = try await collectTask.value
            XCTFail("a gap must fail the collect")
        } catch let err as TransferReceiverError {
            guard case .gap = err else { return XCTFail("expected gap, got \(err)") }
        }
        await teardown(h)
    }

    func testAbandonFailsCollectPromptly() async throws {
        let h = try await boot()
        let transferId = UUID()
        let ref = TransferRef(id: transferId, totalSize: 40 * 1024, sha256: nil)
        await h.receiver.register(deviceId: h.driver.deviceId, ref: ref)

        let collectTask = Task { try await h.receiver.collect(transferId, timeout: .seconds(5)) }
        try await h.driver.send(
            .transfer(.fragment(TransferFragment(transferId: transferId, offset: 0, bytes: Data(count: 4096)))),
            meta: .event
        )
        try await h.driver.send(
            .transfer(.abandon(TransferAbandon(transferId: transferId, reason: "synthetic"))),
            meta: .event
        )
        do {
            _ = try await collectTask.value
            XCTFail("abandon must fail the collect")
        } catch let err as TransferReceiverError {
            guard case .abandoned = err else { return XCTFail("expected abandoned, got \(err)") }
        }
        await teardown(h)
    }
}
