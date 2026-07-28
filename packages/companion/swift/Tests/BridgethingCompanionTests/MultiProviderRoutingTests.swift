import BridgethingGateway
import BridgethingGlue
import BridgethingSchema
import BridgethingTestKit
import XCTest

@testable import BridgethingCompanion

final class MultiProviderRoutingTests: XCTestCase {
    private func boot() async throws -> (BridgethingCompanion, WireDriver, FakeGlue, SecondFakeGlue) {
        let adapter = InMemoryAdapter()
        let companion = BridgethingCompanion(
            adapter: adapter,
            lyricsResolver: FakeLyricsResolver(),
            host: HostInfo(appName: "test-companion", appVersion: "0.0.1", osName: "macOS")
        )
        var behaviors = FakeGlue.Behaviors()
        behaviors.asset = { id in
            id.hasPrefix("fake/") ? AssetBytes(bytes: Data("spotify-art".utf8), mime: "image/jpeg") : nil
        }
        let first = FakeGlue(behaviors: behaviors, uriSchemes: ["spotify"])
        let second = SecondFakeGlue()
        try await companion.attach(first)
        try await companion.attach(second)
        try await companion.start()
        let driver = WireDriver(adapter: adapter)
        await driver.start()
        driver.connect()
        return (companion, driver, first, second)
    }

    func testAnnouncedSchemesAreTheUnionOfAttachedProviders() async throws {
        let (_, driver, _, _) = try await boot()
        let frame = try await driver.waitOutbound { msg in
            if case .capabilities(.announce) = msg.data { return true }
            return false
        }
        guard case let .capabilities(.announce(caps)) = frame.data else {
            return XCTFail("expected a capabilities announce")
        }
        XCTAssertEqual(Set(caps.uriSchemes), ["spotify", "second"])
    }

    func testPlayRoutesByUriScheme() async throws {
        let (_, driver, first, second) = try await boot()
        try await driver.send(
            .player(.play(PlayUri(uri: "second:track:xyz", context: nil)))
        )
        try await Task.sleep(nanoseconds: 200_000_000)
        let played = await second.playedUris
        XCTAssertEqual(played, ["second:track:xyz"])
        XCTAssertFalse(first.calls.contains { if case .play = $0 { return true } else { return false } })
    }

    func testPlayForAnUnclaimedSchemeIsDroppedRatherThanMisrouted() async throws {
        let (_, driver, first, second) = try await boot()
        try await driver.send(
            .player(.play(PlayUri(uri: "tidal:track:xyz", context: nil)))
        )
        try await Task.sleep(nanoseconds: 200_000_000)
        let played = await second.playedUris
        XCTAssertTrue(played.isEmpty)
        XCTAssertFalse(first.calls.contains { if case .play = $0 { return true } else { return false } })
    }

    func testPlayForAnUnclaimedSchemeReportsTheDrop() async throws {
        let (_, driver, _, _) = try await boot()
        try await driver.send(
            .player(.play(PlayUri(uri: "tidal:track:xyz", context: nil)))
        )
        let frame = try await driver.waitOutbound { msg in
            if case .player(.errorEvent) = msg.data { return true }
            return false
        }
        guard case let .player(.errorEvent(reply)) = frame.data,
              case let .schemeUnclaimed(inner) = reply.error
        else {
            return XCTFail("expected a schemeUnclaimed player error, got \(frame.data)")
        }
        XCTAssertEqual(inner.scheme, "tidal")
    }

    func testAssetResolvesFromTheGlueThatMintedTheId() async throws {
        let (_, driver, _, _) = try await boot()
        let reply = try await driver.request(
            .asset(.request(AssetRequest(id: "fake/img/248/abc", requestId: UUID())))
        )
        guard case let .asset(.got(got)) = reply.data else {
            return XCTFail("expected an asset reply, got \(reply.data)")
        }
        XCTAssertEqual(got.id, "fake/img/248/abc")
    }

    func testOnlyOneGlueIsAllowedToAutoResumeOnConnect() async throws {
        let (_, _, first, second) = try await boot()
        try await Task.sleep(nanoseconds: 300_000_000)
        let firstAllowed = first.calls.contains { call in
            if case let .peerConnected(allow) = call { return allow }
            return false
        }
        let secondAllowed = await second.autoResumeAllowed
        XCTAssertFalse(firstAllowed && secondAllowed, "only one provider may resume on connect")
    }
}

private actor SecondFakeGlue: BridgethingGlue {
    static let name = "second"
    static let displayName = "Second"

    nonisolated var capabilities: GlueCapabilities { [] }
    nonisolated var uriSchemes: [String] { ["second"] }
    nonisolated var musicProvider: MusicProvider { .appleMusic }
    nonisolated var lyricsSupported: Bool { false }

    private(set) var playedUris: [String] = []
    private(set) var autoResumeAllowed = false

    func attach(gateway _: BridgethingGateway) async throws {}
    func detach() async {}
    func play(_ uri: PlayUri) async throws { playedUris.append(uri.uri) }
    func handlePeerConnected(allowAutoResume: Bool) async { autoResumeAllowed = allowAutoResume }
}
