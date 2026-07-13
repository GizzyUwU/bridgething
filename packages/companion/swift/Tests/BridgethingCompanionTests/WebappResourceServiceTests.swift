import BridgethingGateway
import BridgethingSchema
import BridgethingTestKit
import Foundation
import XCTest
#if canImport(CryptoKit)
    import CryptoKit
#endif

@testable import BridgethingCompanion

final class WebappResourceServiceTests: XCTestCase {
    private struct Harness {
        let gateway: BridgethingGateway
        let driver: WireDriver
        let receiver: TransferReceiver
        let service: WebappResourceService
        let cacheDir: URL
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
        let cacheDir = FileManager.default.temporaryDirectory.appendingPathComponent("wrs-\(UUID())")
        let service = WebappResourceService(receiver: receiver, cacheDirectory: cacheDir)
        await service.start(gateway: gateway)
        return Harness(gateway: gateway, driver: driver, receiver: receiver, service: service, cacheDir: cacheDir)
    }

    private func teardown(_ h: Harness) async {
        await h.receiver.stop()
        await h.driver.stop()
        await h.gateway.stop()
        try? FileManager.default.removeItem(at: h.cacheDir)
    }

    private func sha256hex(_ data: Data) -> String {
        #if canImport(CryptoKit)
            return SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        #else
            return ""
        #endif
    }

    private func nextResourceRequest(_ driver: WireDriver) async throws -> GatewayToBridgeMsg {
        try await driver.waitOutbound(timeout: .seconds(3)) { m in
            if case .webapp(.resource) = m.data { return true }
            return false
        }
    }

    func testInlineFetchLandsInCache() async throws {
        let h = try await boot()
        let webappId = UUID()
        let body = Data("<html><body>settings</body></html>".utf8)
        let sha = sha256hex(body)

        let fetchTask = Task {
            try await h.service.fetch(deviceId: h.driver.deviceId, webappId: webappId, kind: .settings)
        }

        let reqFrame = try await nextResourceRequest(h.driver)
        guard case let .webapp(.resource(req)) = reqFrame.data else { return XCTFail("expected webapp.resource") }
        XCTAssertEqual(req.id, webappId)
        XCTAssertEqual(req.kind, .settings)
        XCTAssertNil(req.have, "first fetch has nothing cached")

        try await h.driver.send(
            .webapp(.resource(WebappResourceReply(id: webappId, kind: .settings, sha256: sha, mime: "text/html", body: .inline(body)))),
            meta: .response(ResponseMeta(requestId: reqFrame.id))
        )

        let resolved = try await fetchTask.value
        XCTAssertEqual(resolved.sha256, sha)
        XCTAssertEqual(resolved.url.pathExtension, "html")
        XCTAssertEqual(try Data(contentsOf: resolved.url), body)

        await teardown(h)
    }

    func testCachedFetchSendsHaveAndServesCachedFile() async throws {
        let h = try await boot()
        let webappId = UUID()
        let body = Data("<html>v1</html>".utf8)
        let sha = sha256hex(body)

        // first fetch populates the cache.
        let firstTask = Task {
            try await h.service.fetch(deviceId: h.driver.deviceId, webappId: webappId, kind: .settings)
        }
        let firstReq = try await nextResourceRequest(h.driver)
        try await h.driver.send(
            .webapp(.resource(WebappResourceReply(id: webappId, kind: .settings, sha256: sha, mime: "text/html", body: .inline(body)))),
            meta: .response(ResponseMeta(requestId: firstReq.id))
        )
        let firstResolved = try await firstTask.value

        // second fetch must offer the cached sha and accept a body-less reply, serving the cached file.
        let secondTask = Task {
            try await h.service.fetch(deviceId: h.driver.deviceId, webappId: webappId, kind: .settings)
        }
        let secondReq = try await nextResourceRequest(h.driver)
        guard case let .webapp(.resource(req)) = secondReq.data else { return XCTFail("expected webapp.resource") }
        XCTAssertEqual(req.have, sha, "second fetch must send the cached sha as have")
        try await h.driver.send(
            .webapp(.resource(WebappResourceReply(id: webappId, kind: .settings, sha256: sha, mime: "text/html", body: nil))),
            meta: .response(ResponseMeta(requestId: secondReq.id))
        )
        let secondResolved = try await secondTask.value
        XCTAssertEqual(secondResolved.url, firstResolved.url, "a body-less reply must serve the cached file")
        XCTAssertEqual(try Data(contentsOf: secondResolved.url), body)

        await teardown(h)
    }

    func testStreamFetchReassemblesAndCaches() async throws {
        let h = try await boot()
        let webappId = UUID()
        let body = Data((0 ..< 40 * 1024).map { UInt8($0 % 251) })
        let sha = sha256hex(body)
        let transferId = UUID()

        let fetchTask = Task {
            try await h.service.fetch(deviceId: h.driver.deviceId, webappId: webappId, kind: .icon)
        }

        let reqFrame = try await nextResourceRequest(h.driver)
        try await h.driver.send(
            .webapp(.resource(WebappResourceReply(
                id: webappId, kind: .icon, sha256: sha, mime: "image/png",
                body: .stream(TransferRef(id: transferId, totalSize: UInt32(body.count), sha256: sha))
            ))),
            meta: .response(ResponseMeta(requestId: reqFrame.id))
        )

        // the daemon streams fragments right after the reply; the receiver buffers past the register race.
        var offset = 0
        while offset < body.count {
            let end = min(offset + 4096, body.count)
            try await h.driver.send(
                .transfer(.fragment(TransferFragment(transferId: transferId, offset: UInt32(offset), bytes: body.subdata(in: offset ..< end)))),
                meta: .event
            )
            offset = end
        }

        let resolved = try await fetchTask.value
        XCTAssertEqual(resolved.sha256, sha)
        XCTAssertEqual(resolved.url.pathExtension, "png")
        XCTAssertEqual(try Data(contentsOf: resolved.url), body, "streamed resource must reassemble and cache")

        await teardown(h)
    }
}
