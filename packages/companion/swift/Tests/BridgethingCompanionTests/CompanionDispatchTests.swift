import BridgethingGateway
import BridgethingGlue
import BridgethingSchema
import BridgethingTestKit
import XCTest

@testable import BridgethingCompanion

/// Boots the real `BridgethingCompanion` over an `InMemoryAdapter` and drives
/// wire requests through it exactly as the daemon would over the BT transport.
final class CompanionDispatchTests: XCTestCase {
    struct Harness {
        let companion: BridgethingCompanion
        let driver: WireDriver
        let adapter: InMemoryAdapter
        let glue: FakeGlue
    }

    private func boot(glue: FakeGlue = FakeGlue()) async throws -> Harness {
        let adapter = InMemoryAdapter()
        let companion = BridgethingCompanion(
            adapter: adapter,
            lyricsResolver: FakeLyricsResolver(),
            host: HostInfo(appName: "test-companion", appVersion: "0.0.1", osName: "macOS")
        )
        try await companion.setActive(glue)
        try await companion.start()
        let driver = WireDriver(adapter: adapter)
        await driver.start()
        driver.connect()
        return Harness(companion: companion, driver: driver, adapter: adapter, glue: glue)
    }

    func testAnnouncesCapabilitiesOnConnect() async throws {
        let h = try await boot()
        let frame = try await h.driver.waitOutbound { msg in
            if case .capabilities(.announce) = msg.data { return true }
            return false
        }
        guard case let .capabilities(.announce(caps)) = frame.data else {
            return XCTFail("expected capabilities announce, got \(frame.data)")
        }
        XCTAssertEqual(caps.gateway.appName, "test-companion")
        XCTAssertEqual(caps.musicProvider, .none)
        await h.companion.stop()
    }

    func testLibraryBrowseRoutesToActiveGlue() async throws {
        var behaviors = FakeGlue.Behaviors()
        let canned = BrowseResult(
            entries: [.folder(BrowseFolder(
                nodeId: "root/playlists", title: "Playlists",
                subtitle: nil, artworkId: nil, total: 3, previewChildren: nil
            ))],
            total: 1,
            hasMore: false
        )
        behaviors.browse = { _ in canned }
        let h = try await boot(glue: FakeGlue(behaviors: behaviors))

        let resp = try await h.driver.request(
            .library(.browse(LibraryBrowseRequest(nodeId: nil, limit: 20, offset: 0))),
            timeout: .seconds(3)
        )
        guard case let .library(.browseReply(reply)) = resp.data else {
            return XCTFail("expected browseReply, got \(resp.data)")
        }
        XCTAssertEqual(reply.result.entries.count, 1)
        XCTAssertTrue(h.glue.calls.contains(.browse))
        await h.companion.stop()
    }

    func testLibrarySearchUnimplementedGlueReturnsProtocolError() async throws {
        let h = try await boot(glue: FakeGlue())
        let resp = try await h.driver.request(
            .library(.search(LibrarySearchRequest(query: "daft punk", kinds: nil, limit: 10, offset: 0))),
            timeout: .seconds(3)
        )
        guard case let .error(wireError) = resp.data else {
            return XCTFail("expected protocol error, got \(resp.data)")
        }
        if case .unimplemented = wireError {} else {
            XCTFail("expected .unimplemented, got \(wireError)")
        }
        await h.companion.stop()
    }

    func testNetFetchRealOpenMeteoRequest() async throws {
        let h = try await boot()
        let url = "https://api.open-meteo.com/v1/forecast?latitude=40.71&longitude=-74.0&current=temperature_2m,weather_code"
        let req = NetFetchRequest(url: url, method: .get, headers: [], body: nil, timeoutMs: 12000, redirect: .follow)

        let resp: GatewayToBridgeMsg
        do {
            resp = try await h.driver.request(.net(.fetch(NetFetchRequestMsg(request: req))), timeout: .seconds(15))
        } catch {
            await h.companion.stop()
            throw XCTSkip("network unavailable for net.fetch integration test: \(error)")
        }

        switch resp.data {
        case let .net(.fetchReply(reply)):
            XCTAssertEqual(reply.response.status, 200)
            let json = try JSONSerialization.jsonObject(with: reply.response.body) as? [String: Any]
            XCTAssertNotNil(json?["current"], "open-meteo body should carry a `current` block")
        case let .net(.fetchErrorReply(err)):
            XCTFail("companion net.fetch returned an error: \(err)")
        default:
            XCTFail("unexpected response \(resp.data)")
        }
        await h.companion.stop()
    }
}
