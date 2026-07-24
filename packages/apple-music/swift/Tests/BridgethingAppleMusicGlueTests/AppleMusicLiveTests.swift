import BridgethingGlue
import Foundation
import XCTest

@testable import BridgethingAppleMusicGlue

#if canImport(MusicKit)
    import MusicKit

    final class AppleMusicLiveTests: XCTestCase {
        private static let heroEdge = 248

        private func requireLive() async throws -> MusicKitLibrary {
            try XCTSkipUnless(
                ProcessInfo.processInfo.environment["BRIDGETHING_APPLEMUSIC_LIVE"] == "1",
                "set BRIDGETHING_APPLEMUSIC_LIVE=1 to run live Apple Music validation"
            )
            let status = await MusicKitAuth().currentStatus()
            guard status == .authorized else {
                XCTFail("media library access is \(status); run scripts/apple-music-authorize once from this terminal")
                throw AmPlayerError.unavailable
            }
            return MusicKitLibrary()
        }

        private func skipIfTokenUnavailable(_ error: Error, _ what: String) throws {
            let text = String(describing: error).lowercased()
            if text.contains("token") || text.contains("401") || text.contains("403") {
                throw XCTSkip("\(what) needs a developer token the unsigned test host cannot mint: \(error)")
            }
        }

        private func fetchArt(_ item: AmItem) async throws -> Data {
            guard let template = item.artworkUrl else {
                throw XCTSkip("\(item.uri) has no artwork")
            }
            let glue = AppleMusicGlue()
            let codec = ImageAssetCodec(namespace: "applemusic/img/")
            let id = codec.assetId(url: sizedArtworkUrl(template, edge: Self.heroEdge), maxEdge: Self.heroEdge)
            let bytes = try await glue.asset(id: XCTUnwrap(id))
            return try XCTUnwrap(bytes, "asset pipeline returned nil for \(item.uri)").bytes
        }

        func testLibraryPlaylistDrilldown() async throws {
            let library = try await requireLive()
            let playlists = try await library.libraryPlaylists(limit: 10, offset: 0)
            try XCTSkipIf(playlists.items.isEmpty, "account has no library playlists")

            for item in playlists.items {
                let parsed = try XCTUnwrap(AmUri.parse(item.uri))
                XCTAssertEqual(parsed.kind, .playlist)
                XCTAssertTrue(isLibraryId(parsed.id), "library playlist id \(parsed.id) not classified as library")
            }
            for item in playlists.items {
                let children = try await library.children(of: item.uri, limit: 25, offset: 0)
                if children.items.isEmpty { continue }
                XCTAssertTrue(children.items.allSatisfy { $0.kind == .song })
                return
            }
            XCTFail("no library playlist returned any tracks")
        }

        func testLibraryAlbumDrilldown() async throws {
            let library = try await requireLive()
            let albums = try await library.libraryAlbums(limit: 10, offset: 0)
            try XCTSkipIf(albums.items.isEmpty, "account has no library albums")

            for item in albums.items {
                let parsed = try XCTUnwrap(AmUri.parse(item.uri))
                XCTAssertTrue(isLibraryId(parsed.id), "library album id \(parsed.id) not classified as library")
            }
            for item in albums.items {
                let children = try await library.children(of: item.uri, limit: 25, offset: 0)
                if children.items.isEmpty { continue }
                XCTAssertTrue(children.items.allSatisfy { $0.kind == .song })
                return
            }
            XCTFail("no library album returned any tracks")
        }

        func testLibraryArtistDrilldown() async throws {
            let library = try await requireLive()
            let artists = try await library.libraryArtists(limit: 10, offset: 0)
            try XCTSkipIf(artists.items.isEmpty, "account has no library artists")
            for item in artists.items {
                let children = try await library.children(of: item.uri, limit: 25, offset: 0)
                if children.items.isEmpty { continue }
                XCTAssertTrue(children.items.allSatisfy { $0.kind == .album })
                return
            }
            XCTFail("no library artist returned any albums")
        }

        func testLibraryArtworkFetch() async throws {
            let library = try await requireLive()
            let pages = try await [
                library.libraryPlaylists(limit: 10, offset: 0),
                library.libraryAlbums(limit: 10, offset: 0),
            ]
            let withArt = pages.flatMap(\.items).filter { $0.artworkUrl != nil }
            try XCTSkipIf(withArt.isEmpty, "no library items with artwork")

            var fetched = 0
            for item in withArt.prefix(4) {
                let data = try await fetchArt(item)
                XCTAssertGreaterThan(data.count, 500, "suspiciously small artwork for \(item.uri)")
                fetched += 1
            }
            XCTAssertGreaterThan(fetched, 0)
        }

        func testCatalogSearchAndArtworkFetch() async throws {
            let library = try await requireLive()
            do {
                let results = try await library.search(query: "Daft Punk", limit: 5)
                XCTAssertFalse(results.songs.isEmpty)
                XCTAssertFalse(results.albums.isEmpty)
                let item = try XCTUnwrap((results.albums + results.songs).first { $0.artworkUrl != nil })
                XCTAssertTrue(item.artworkUrl!.contains("{w}x{h}"), "catalog template missing size tokens: \(item.artworkUrl!)")
                let data = try await fetchArt(item)
                XCTAssertGreaterThan(data.count, 500)
            } catch {
                try skipIfTokenUnavailable(error, "catalog search")
                throw error
            }
        }

        func testRecommendationsAndRecents() async throws {
            let library = try await requireLive()
            do {
                let rails = try await library.recommendations()
                XCTAssertFalse(rails.isEmpty)
                _ = try await library.recentlyPlayed(limit: 10, offset: 0)
            } catch {
                try skipIfTokenUnavailable(error, "recommendations")
                throw error
            }
        }

        func testFavoritesRead() async throws {
            let library = try await requireLive()
            do {
                let results = try await library.search(query: "Daft Punk", limit: 3)
                let uris = results.songs.map(\.uri)
                try XCTSkipIf(uris.isEmpty, "no catalog songs to probe")
                let flags = try await library.isFavorite(uris: uris)
                XCTAssertEqual(flags.count, uris.count)
            } catch {
                try skipIfTokenUnavailable(error, "favorites read")
                throw error
            }
        }
    }

#endif
