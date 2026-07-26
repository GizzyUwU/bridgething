import BridgethingGateway
import BridgethingGlue
import BridgethingSchema
import BridgethingTestKit
import XCTest

@testable import BridgethingCompanion

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
        try await companion.attach(glue)
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
            .library(.browse(LibraryBrowseRequest(nodeId: nil, limit: 20, offset: 0, sections: nil, preview: nil))),
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

    func testDeviceNicknameChangedPatchesCachedMeta() async throws {
        let h = try await boot()
        let ota = await h.companion.ota

        try await h.driver.send(.version(Self.testMeta(nickname: nil)), meta: .event)
        guard await Self.waitUntil({ await ota.meta(deviceId: h.driver.deviceId) != nil }) else {
            await h.companion.stop()
            return XCTFail("version announce never landed in the ota meta cache")
        }

        try await h.driver.send(
            .system(.deviceNicknameChanged(DeviceNicknameReply(nickname: "garage thing"))),
            meta: .event
        )
        guard await Self.waitUntil({ await ota.meta(deviceId: h.driver.deviceId)?.nickname == "garage thing" }) else {
            await h.companion.stop()
            return XCTFail("nickname change never patched the cached meta")
        }

        let driverDeviceId = await h.driver.deviceId
        var updates = ota.metaChanged.makeAsyncIterator()
        let announced = await updates.next()
        XCTAssertEqual(announced?.deviceId, driverDeviceId)
        XCTAssertNil(announced?.meta.nickname)
        let patched = await updates.next()
        XCTAssertEqual(patched?.meta.nickname, "garage thing")
        XCTAssertEqual(patched?.meta.serialNumber, "SN-TEST-0001")
        await h.companion.stop()
    }

    func testAwaitMetaResolvesWhenVersionLandsAfterTheCall() async throws {
        let h = try await boot()
        let ota = await h.companion.ota
        let deviceId = await h.driver.deviceId

        let cachedBefore = await ota.meta(deviceId: deviceId)
        XCTAssertNil(cachedBefore, "precondition: meta must not be cached yet")

        async let pending = ota.awaitMeta(deviceId: deviceId)
        try await h.driver.send(.version(Self.testMeta(nickname: nil)), meta: .event)

        let resolved = await pending
        XCTAssertEqual(resolved?.serialNumber, "SN-TEST-0001")
        await h.companion.stop()
    }

    func testAwaitMetaTimesOutWhenNoVersionEverArrives() async throws {
        let h = try await boot()
        let ota = await h.companion.ota
        let deviceId = await h.driver.deviceId

        let resolved = await ota.awaitMeta(deviceId: deviceId, timeoutNanos: 200_000_000)
        XCTAssertNil(resolved, "awaitMeta must give up rather than hang the pairing flow")
        await h.companion.stop()
    }

    private static func waitUntil(
        deadline: Duration = .seconds(5),
        _ predicate: () async -> Bool
    ) async -> Bool {
        let start = ContinuousClock.now
        while ContinuousClock.now - start < deadline {
            if await predicate() { return true }
            try? await Task.sleep(for: .milliseconds(20))
        }
        return await predicate()
    }

    private static func testMeta(nickname: String?) -> BridgeThingMeta {
        BridgeThingMeta(
            bridgethingVersion: "v0.0.1",
            libbridgethingVersion: "0.0.1",
            appName: "bridgething",
            nickname: nickname,
            appVersion: "0.0.1",
            daemonSha256: nil,
            osName: "superbird",
            osVersion: "2026.05.0",
            osDescription: "test image",
            btMac: "AA:BB:CC:DD:EE:FF",
            serialNumber: "SN-TEST-0001",
            fccId: "fcc",
            icId: "ic",
            modelName: "Car Thing",
            channel: "dev",
            imageVariant: "dev",
            imageVersion: "2026.05.0",
            imageBuildId: "build",
            imageBuildDate: "2026-05-01",
            imageDistro: "yocto",
            imageMachine: "superbird",
            discord: "",
            credits: ""
        )
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
