import BridgethingGateway
import BridgethingGlue
import BridgethingLyrics
import BridgethingSchema
import BridgethingTestKit
import Foundation
import XCTest

@testable import BridgethingCompanion

/// Asset / lyrics / library-breadth dispatch coverage.
final class AssetLyricsDispatchTests: XCTestCase {
    struct Harness {
        let companion: BridgethingCompanion
        let driver: WireDriver
        let glue: FakeGlue
    }

    private func boot(
        glue: FakeGlue = FakeGlue(),
        lyricsResolver: any LyricsResolver = FakeLyricsResolver()
    ) async throws -> Harness {
        let adapter = InMemoryAdapter()
        let companion = BridgethingCompanion(
            adapter: adapter,
            lyricsResolver: lyricsResolver,
            host: HostInfo(appName: "test-companion", appVersion: "0.0.1", osName: "macOS")
        )
        try await companion.setActive(glue)
        try await companion.start()
        let driver = WireDriver(adapter: adapter)
        await driver.start()
        driver.connect()
        return Harness(companion: companion, driver: driver, glue: glue)
    }

    // MARK: - asset

    func testAssetResolveReturnsBytes() async throws {
        var behaviors = FakeGlue.Behaviors()
        let payload = Data([0x89, 0x50, 0x4E, 0x47])
        behaviors.asset = { id in
            XCTAssertEqual(id, "art:track:1")
            return AssetBytes(bytes: payload, mime: "image/png")
        }
        let h = try await boot(glue: FakeGlue(behaviors: behaviors))

        let resp = try await h.driver.request(
            .asset(.request(AssetRequest(id: "art:track:1", requestId: UUID()))),
            timeout: .seconds(3)
        )
        guard case let .asset(.got(reply)) = resp.data else {
            return XCTFail("expected asset got, got \(resp.data)")
        }
        XCTAssertEqual(reply.id, "art:track:1")
        XCTAssertEqual(reply.bytes, payload)
        XCTAssertEqual(reply.mime, "image/png")
        XCTAssertTrue(h.glue.calls.contains(.asset("art:track:1")))
        await h.companion.stop()
    }

    func testAssetMissReturnsNotFound() async throws {
        // FakeGlue with no asset behavior returns nil -> notFound.
        let h = try await boot()
        let resp = try await h.driver.request(
            .asset(.request(AssetRequest(id: "art:missing", requestId: UUID()))),
            timeout: .seconds(3)
        )
        guard case let .asset(.notFound(reply)) = resp.data else {
            return XCTFail("expected asset notFound, got \(resp.data)")
        }
        XCTAssertEqual(reply.id, "art:missing")
        await h.companion.stop()
    }

    // MARK: - lyrics

    func testLyricsFallsThroughToResolver() async throws {
        // FakeGlue.lyrics returns nil; the resolver chain supplies the hit.
        let canned = BridgethingLyrics.Lyrics(
            synced: [BridgethingLyrics.LyricLine(startMs: 0, text: "one more time")],
            plain: nil,
            source: "fake-resolver"
        )
        let h = try await boot(lyricsResolver: FakeLyricsResolver(canned: canned))

        let resp = try await h.driver.request(
            .lyrics(.get(LyricsRequest(track: TrackIdentity(
                artist: "Daft Punk", track: "One More Time", album: nil, durationMs: nil, isrc: nil
            )))),
            timeout: .seconds(3)
        )
        guard case let .lyrics(.lyricsReply(reply)) = resp.data else {
            return XCTFail("expected lyricsReply, got \(resp.data)")
        }
        XCTAssertEqual(reply.lyrics?.source, "fake-resolver")
        XCTAssertEqual(reply.lyrics?.synced?.first?.text, "one more time")
        await h.companion.stop()
    }

    func testLyricsNoHitReturnsNilReply() async throws {
        // Neither glue nor resolver has lyrics -> a reply carrying lyrics == nil
        let h = try await boot(lyricsResolver: FakeLyricsResolver(canned: nil))
        let resp = try await h.driver.request(
            .lyrics(.get(LyricsRequest(track: TrackIdentity(
                artist: "Unknown", track: "Nope", album: nil, durationMs: nil, isrc: nil
            )))),
            timeout: .seconds(3)
        )
        guard case let .lyrics(.lyricsReply(reply)) = resp.data else {
            return XCTFail("expected lyricsReply, got \(resp.data)")
        }
        XCTAssertNil(reply.lyrics)
        await h.companion.stop()
    }

    // MARK: - library breadth

    func testLibrarySearchRoutesToActiveGlue() async throws {
        var behaviors = FakeGlue.Behaviors()
        behaviors.search = { req in
            XCTAssertEqual(req.query, "daft punk")
            return SearchResult(items: [], kinds: [.track], total: 0, hasMore: false)
        }
        let h = try await boot(glue: FakeGlue(behaviors: behaviors))

        let resp = try await h.driver.request(
            .library(.search(LibrarySearchRequest(query: "daft punk", kinds: nil, limit: 10, offset: 0))),
            timeout: .seconds(3)
        )
        guard case let .library(.searchReply(reply)) = resp.data else {
            return XCTFail("expected searchReply, got \(resp.data)")
        }
        XCTAssertEqual(reply.result.kinds, [.track])
        XCTAssertTrue(h.glue.calls.contains(.search("daft punk")))
        await h.companion.stop()
    }

    func testLibraryFavoritesListRoutesToActiveGlue() async throws {
        var behaviors = FakeGlue.Behaviors()
        behaviors.favoritesList = { _ in
            FavoritesPage(items: [], total: 42, hasMore: true)
        }
        let h = try await boot(glue: FakeGlue(behaviors: behaviors))

        let resp = try await h.driver.request(
            .library(.favoritesList(LibraryFavoritesListRequest(limit: 20, offset: 0))),
            timeout: .seconds(3)
        )
        guard case let .library(.favoritesListReply(reply)) = resp.data else {
            return XCTFail("expected favoritesListReply, got \(resp.data)")
        }
        XCTAssertEqual(reply.page.total, 42)
        XCTAssertTrue(reply.page.hasMore)
        XCTAssertTrue(h.glue.calls.contains(.favoritesList))
        await h.companion.stop()
    }
}
