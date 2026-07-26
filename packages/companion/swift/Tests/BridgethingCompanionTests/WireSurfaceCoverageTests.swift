import BridgethingGateway
import BridgethingGlue
import BridgethingSchema
import BridgethingTestKit
import Foundation
import XCTest

@testable import BridgethingCompanion

final class WireSurfaceCoverageTests: XCTestCase {
    private func boot() async throws -> (BridgethingCompanion, WireDriver) {
        let adapter = InMemoryAdapter()
        let companion = BridgethingCompanion(
            adapter: adapter,
            lyricsResolver: FakeLyricsResolver(),
            host: HostInfo(appName: "coverage", appVersion: "0.0.1", osName: "macOS")
        )
        try await companion.attach(FakeGlue())
        try await companion.start()
        let driver = WireDriver(adapter: adapter)
        await driver.start()
        driver.connect()
        return (companion, driver)
    }

    private static let probes: [String: BridgeToGatewayMsgData] = [
        "asset.request": .asset(.request(AssetRequest(id: "probe", requestId: UUID()))),
        "library.browse": .library(.browse(LibraryBrowseRequest(nodeId: nil, limit: 1, offset: 0, sections: nil, preview: nil))),
        "library.resolveContext": .library(.resolveContext(LibraryResolveContextRequest(uri: "x"))),
        "library.search": .library(.search(LibrarySearchRequest(query: "x", kinds: nil, limit: 1, offset: 0))),
        "library.recommendations": .library(.recommendations(
            LibraryRecommendationsRequest(seeds: [], kind: nil, limit: 1, offset: 0)
        )),
        "library.favoritesList": .library(.favoritesList(LibraryFavoritesListRequest(limit: 1, offset: 0))),
        "library.favoritesContains": .library(.favoritesContains(LibraryFavoritesContainsRequest(uris: ["x"]))),
        "lyrics.get": .lyrics(.get(LyricsRequest(track: TrackIdentity(
            artist: "a", track: "b", album: nil, durationMs: nil, isrc: nil
        )))),
        "tunnel.open": .tunnel(.open(TunnelOpen(tunnelId: UUID(), host: "127.0.0.1", port: 1))),
        "system.keepalive": .system(.keepalive(KeepalivePing(seq: 0))),
    ]

    private static let handledElsewhere: Set<String> = [
        "net.fetch",
        "geo.getOnce",
        "net.wsOpen",
        "system.otaAssetRange",
    ]

    private static let knownUnimplemented: Set<String> = [
        "phone.stateGet",
    ]

    func testEveryInboundRequestIsClassified() {
        let accounted = Set(Self.probes.keys)
            .union(Self.handledElsewhere)
            .union(Self.knownUnimplemented)
        XCTAssertEqual(
            Set(WireSurfaceManifest.inboundRequests),
            accounted,
            "inbound request surface drift: classify each id as a probe, handledElsewhere, or knownUnimplemented"
        )
    }

    func testProbedRequestsGetAReplyAndNeverHang() async throws {
        let (companion, driver) = try await boot()
        defer { Task { await companion.stop() } }
        for (id, data) in Self.probes {
            do {
                _ = try await driver.request(data, timeout: .seconds(3))
            } catch {
                XCTFail("inbound request `\(id)` did not reply within the timeout (silent hang): \(error)")
            }
        }
        await companion.stop()
    }

    private static let accountedCommandsAndEvents: Set<String> = [
        // player
        "player.play", "player.pause", "player.queue", "player.resume",
        "player.seekTo", "player.setCrossfade", "player.setRepeat", "player.setShuffle",
        "player.setSpeed", "player.skipNext", "player.skipPrev", "player.skipToIndex",
        "player.transferTo",
        // library favorites
        "library.favoritesSet", "library.favoritesSetMany", "library.favoritesToggle",
        // geo
        "geo.watch", "geo.unwatch",
        // net
        "net.streamOpen", "net.streamCancel", "net.wsClose", "net.wsSend",
        // notifications
        "notifications.ancsAuthStateChanged", "notifications.invokePositive", "notifications.invokeNegative",
        // audio
        "audio.volumeUp", "audio.volumeDown", "audio.setVolume", "audio.muteToggle",
        "audio.setMute", "audio.tts", "audio.ttsCancel", "audio.ttsCancelAll", "audio.earcon",
        // phone
        "phone.answer", "phone.accept", "phone.decline", "phone.end", "phone.endTyped",
        "phone.hold", "phone.unhold", "phone.initiate", "phone.swap", "phone.merge",
        "phone.mute", "phone.dtmf",
        // tunnel
        "tunnel.data", "tunnel.close",
        // transfer
        "transfer.ack", "transfer.fragment", "transfer.abandon",
        // system OTA
        "system.otaProgress", "system.otaError", "system.otaBeginAck",
        "system.otaBeginRejected", "system.otaAssetRangeAbandon",
        // system nicknames
        "system.deviceNickname", "system.deviceNicknameChanged", "system.deviceNicknameRejected",
        // system logs
        "system.logEntry", "system.logsTailReply", "system.logsSubscribeReply",
        // voice
        "voice.streamOpen", "voice.frame", "voice.streamClose", "voice.dispatched", "voice.dispatchFailed",
        // webapp
        "webapp.webapps", "webapp.active", "webapp.switched", "webapp.uninstalled",
        "webapp.webappError", "webapp.resource", "webapp.configGet",
        "webapp.configList", "webapp.configAck", "webapp.webappInstalled", "webapp.activeChanged",
        "webapp.docGet", "webapp.docList", "webapp.docAck", "webapp.docChanged",
        // forward
        "forward.text", "forward.binary", "forward.json",
    ]

    func testEveryInboundCommandOrEventIsAccountedFor() {
        let manifest = Set(WireSurfaceManifest.inboundCommands).union(WireSurfaceManifest.inboundEvents)
        XCTAssertEqual(
            manifest,
            Self.accountedCommandsAndEvents,
            "inbound command/event surface drift: a new variant appeared (or one was removed) - classify it"
        )
    }
}
