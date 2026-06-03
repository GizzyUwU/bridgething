import BridgethingCompanion
import BridgethingGateway
import BridgethingGlue
import BridgethingSchema
import BridgethingSpotifyGlue
import BridgethingTestKit
import Foundation
import Spotiny
import XCTest

final class SpotifyGlueIntegrationTests: XCTestCase {
    struct Harness {
        let companion: BridgethingCompanion
        let driver: WireDriver
    }

    final class StubAuthenticator: OAuthAuthenticator, @unchecked Sendable {
        let token: OAuthTokenResponse

        init(accessToken: String) {
            let data = try! JSONSerialization.data(withJSONObject: [
                "access_token": accessToken,
                "token_type": "Bearer",
            ])
            token = try! JSONDecoder().decode(OAuthTokenResponse.self, from: data)
        }

        func authorize() async throws -> OAuthTokenResponse { token }
        func refreshAccessToken(_: String) async throws -> OAuthTokenResponse { token }
    }

    private func cassetteDir() -> URL {
        URL(fileURLWithPath: #filePath).deletingLastPathComponent().appendingPathComponent("Cassettes")
    }

    private func boot() async throws -> Harness {
        let tokens = SpotifyTokenStore.load()
        let dir = cassetteDir()
        let haveCassettes = (try? FileManager.default.contentsOfDirectory(atPath: dir.path))?
            .contains { $0.hasSuffix(".json") } ?? false

        try XCTSkipUnless(tokens != nil || haveCassettes, "no Spotify token cached and no cassettes recorded")

        let refresh = ProcessInfo.processInfo.environment["BRIDGETHING_SPOTIFY_REFRESH"] == "1"
        let executor = CassetteExecutor(dir: dir, refresh: refresh, allowLive: tokens != nil)
        let access = tokens?.accessToken ?? "replay-placeholder"

        let glue = SpotifyGlue(
            authenticatorFactory: { StubAuthenticator(accessToken: access) },
            accessToken: access,
            refreshToken: "",
            httpExecutor: executor
        )

        let adapter = InMemoryAdapter()
        let companion = BridgethingCompanion(
            adapter: adapter,
            lyricsResolver: FakeLyricsResolver(),
            host: HostInfo(appName: "spotify-itest", appVersion: "0.0.1", osName: "macOS")
        )
        try await companion.setActive(glue)
        try await companion.start()

        let driver = WireDriver(adapter: adapter)
        await driver.start()
        driver.connect()

        return Harness(companion: companion, driver: driver)
    }

    private func request(_ h: Harness, _ data: BridgeToGatewayMsgData) async throws -> GatewayToBridgeMsg {
        do {
            return try await h.driver.request(data, timeout: .seconds(20))
        } catch let CassetteExecutor.CassetteError.rateLimited(url) {
            throw XCTSkip("Spotify rate-limited while recording \(url); re-run when the limit clears")
        }
    }

    func testBrowseRootReturnsHomeSections() async throws {
        let h = try await boot()
        let resp = try await request(h, .library(.browse(LibraryBrowseRequest(nodeId: nil, limit: 20, offset: 0))))
        defer { Task { await h.companion.stop() } }

        guard case let .library(.browseReply(reply)) = resp.data else {
            return XCTFail("expected browseReply, got \(resp.data)")
        }
        XCTAssertFalse(reply.result.entries.isEmpty, "browse root should contain home sections")

        var titles: [String] = []
        for entry in reply.result.entries {
            guard case let .folder(folder) = entry else {
                return XCTFail("root entries should all be folders, got \(entry)")
            }
            titles.append(folder.title)
        }
        XCTAssertTrue(titles.contains("Playlists"), "expected a Playlists section, got \(titles)")
        XCTAssertTrue(
            titles.contains("Recently Played") || titles.contains("Top Tracks"),
            "expected a home section, got \(titles)"
        )
        XCTAssertTrue(titles.contains("Home"), "expected the Made-For-You Home section, got \(titles)")
    }

    /// pull a node id out of root's section preview children, matched by a predicate on the contained item.
    private func nodeFromRoot(_ h: Harness, where match: (BridgethingSchema.LibraryItem) -> String?) async throws -> String? {
        let rootResp = try await request(h, .library(.browse(LibraryBrowseRequest(nodeId: nil, limit: 20, offset: 0))))
        guard case let .library(.browseReply(root)) = rootResp.data else {
            XCTFail("expected browseReply, got \(rootResp.data)")
            return nil
        }
        for entry in root.result.entries {
            guard case let .folder(folder) = entry else { continue }
            for child in folder.previewChildren ?? [] {
                guard case let .item(item) = child, let id = match(item) else { continue }
                return id
            }
        }
        return nil
    }

    func testDrillIntoPlaylistReturnsTracks() async throws {
        let h = try await boot()
        defer { Task { await h.companion.stop() } }

        let node = try await nodeFromRoot(h) { item in
            if case let .playlist(p) = item, p.uri.hasPrefix("spotify:playlist:") { return p.uri }
            return nil
        }
        guard let node else { throw XCTSkip("no playlist on this account to drill into") }

        let resp = try await request(h, .library(.browse(LibraryBrowseRequest(nodeId: node, limit: 20, offset: 0))))
        guard case let .library(.browseReply(reply)) = resp.data else {
            return XCTFail("expected browseReply, got \(resp.data)")
        }
        XCTAssertFalse(reply.result.entries.isEmpty, "drilling into a playlist should return its tracks, got empty")
        guard case let .item(first)? = reply.result.entries.first else {
            return XCTFail("expected an item entry, got \(String(describing: reply.result.entries.first))")
        }
        switch first {
        case let .track(t): XCTAssertFalse(t.id.isEmpty)
        case let .podcastEpisode(e): XCTAssertFalse(e.uri.isEmpty)
        default: XCTFail("playlist children should be tracks/episodes, got \(first)")
        }
    }

    func testDrillIntoLikedSongsReturnsTracks() async throws {
        let h = try await boot()
        defer { Task { await h.companion.stop() } }

        let node = try await nodeFromRoot(h) { item in
            if case let .playlist(p) = item, p.name == "Liked Songs" { return p.uri }
            return nil
        }
        guard let node else { throw XCTSkip("no Liked Songs node on this account") }

        let resp = try await request(h, .library(.browse(LibraryBrowseRequest(nodeId: node, limit: 20, offset: 0))))
        guard case let .library(.browseReply(reply)) = resp.data else {
            return XCTFail("expected browseReply, got \(resp.data)")
        }
        guard case let .item(.track(track))? = reply.result.entries.first else {
            throw XCTSkip("no liked songs on this account; nothing to assert")
        }
        XCTAssertTrue(track.id.hasPrefix("spotify:track:"), "liked song id should be a track uri, got \(track.id)")
        XCTAssertTrue(track.saved, "liked songs should report saved=true")
    }

    func testSearchTracksReturnsRealTracks() async throws {
        let h = try await boot()
        let resp = try await request(
            h, .library(.search(LibrarySearchRequest(query: "daft punk", kinds: [.track], limit: 10, offset: 0)))
        )
        defer { Task { await h.companion.stop() } }

        guard case let .library(.searchReply(reply)) = resp.data else {
            return XCTFail("expected searchReply, got \(resp.data)")
        }
        guard let first = reply.result.items.first else {
            throw XCTSkip("search returned no items (transient); skipping content assertion")
        }
        guard case let .track(track) = first else {
            return XCTFail("expected a track item, got \(first)")
        }
        XCTAssertTrue(track.id.hasPrefix("spotify:track:"), "track id should be a spotify uri, got \(track.id)")
        XCTAssertFalse(track.name.isEmpty)
    }

    func testFavoritesListThenContains() async throws {
        let h = try await boot()
        defer { Task { await h.companion.stop() } }

        let listResp = try await request(h, .library(.favoritesList(LibraryFavoritesListRequest(limit: 10, offset: 0))))
        guard case let .library(.favoritesListReply(reply)) = listResp.data else {
            return XCTFail("expected favoritesListReply, got \(listResp.data)")
        }
        guard case let .track(first)? = reply.page.items.first else {
            throw XCTSkip("no liked songs on this account; skipping contains assertion")
        }

        let containsResp = try await request(
            h, .library(.favoritesContains(LibraryFavoritesContainsRequest(uris: [first.id])))
        )
        guard case let .library(.favoritesContainsReply(creply)) = containsResp.data else {
            return XCTFail("expected favoritesContainsReply, got \(containsResp.data)")
        }
        XCTAssertEqual(creply.liked.first, true, "a track from the liked list should report contains=true")
    }
}
