import BridgethingCompanion
import BridgethingGateway
import BridgethingGlue
import BridgethingSchema
import BridgethingTestKit
import Foundation
import XCTest

@testable import BridgethingAppleMusicGlue

final class AppleMusicGlueTests: XCTestCase {
    final class FakeAuth: AppleMusicAuthProviding, @unchecked Sendable {
        var status: AmAuthStatus
        var requestResult: AmAuthStatus
        var subscribed: Bool?
        init(status: AmAuthStatus = .authorized, requestResult: AmAuthStatus = .authorized, subscribed: Bool? = true) {
            self.status = status
            self.requestResult = requestResult
            self.subscribed = subscribed
        }

        func currentStatus() async -> AmAuthStatus { status }
        func requestAuthorization() async -> AmAuthStatus { requestResult }
        func canPlayCatalogContent() async -> Bool? { subscribed }
    }

    final class FakePlayer: AppleMusicPlayerProviding, @unchecked Sendable {
        private let lock = NSLock()
        private var continuation: AsyncStream<Void>.Continuation?
        var snapshot = AmPlayerSnapshot(entry: nil, playing: false, positionMs: 0, shuffle: false, repeatMode: .off)
        var otherAudio = false
        var playContextCalls: [(context: String, startAt: String?)] = []
        var queueInserts: [(uri: String, next: Bool)] = []
        var playCount = 0
        var pauseCount = 0
        var nextCount = 0
        var prevCount = 0
        var seeks: [UInt32] = []
        var shuffles: [Bool] = []
        var repeats: [AmRepeatMode] = []

        func push(_ snap: AmPlayerSnapshot) {
            lock.withLock { snapshot = snap }
            lock.withLock { continuation }?.yield(())
        }

        func changes() -> AsyncStream<Void> {
            AsyncStream { cont in lock.withLock { continuation = cont } }
        }

        func currentSnapshot() async -> AmPlayerSnapshot { lock.withLock { snapshot } }
        func play(contextUri: String, startAtUri: String?) async throws {
            lock.withLock { playContextCalls.append((contextUri, startAtUri)) }
        }

        func queueInsert(uri: String, next: Bool) async throws { lock.withLock { queueInserts.append((uri, next)) } }
        func play() async throws { lock.withLock { playCount += 1 } }
        func pause() async throws { lock.withLock { pauseCount += 1 } }
        func skipNext() async throws { lock.withLock { nextCount += 1 } }
        func skipPrev() async throws { lock.withLock { prevCount += 1 } }
        func seek(toMs ms: UInt32) async throws { lock.withLock { seeks.append(ms) } }
        func setShuffle(_ on: Bool) async throws { lock.withLock { shuffles.append(on) } }
        func setRepeat(_ mode: AmRepeatMode) async throws { lock.withLock { repeats.append(mode) } }
        func isOtherAudioPlaying() async -> Bool { otherAudio }
    }

    final class FakeLibrary: AppleMusicLibraryProviding, @unchecked Sendable {
        private let lock = NSLock()
        var playlists = AmPage(items: [], total: nil, hasMore: false)
        var albums = AmPage(items: [], total: nil, hasMore: false)
        var artists = AmPage(items: [], total: nil, hasMore: false)
        var recents = AmPage(items: [], total: nil, hasMore: false)
        var songs = AmPage(items: [], total: nil, hasMore: false)
        var rails: [AmShelf] = []
        var childrenPage = AmPage(items: [], total: nil, hasMore: false)
        var resolved = AmItem(uri: "applemusic:playlist:1", kind: .playlist, title: "Ctx")
        var searchResults = AmSearchResults(songs: [], albums: [], artists: [], playlists: [])
        var favoriteState: [String: Bool] = [:]
        var favoriteWrites: [(String, Bool)] = []
        var childrenCalls: [(uri: String, limit: UInt32, offset: UInt32)] = []
        var playlistCalls: [(limit: UInt32, offset: UInt32)] = []

        func libraryPlaylists(limit: UInt32, offset: UInt32) async throws -> AmPage {
            lock.withLock { playlistCalls.append((limit, offset)) }
            return playlists
        }

        func libraryAlbums(limit _: UInt32, offset _: UInt32) async throws -> AmPage { albums }
        func libraryArtists(limit _: UInt32, offset _: UInt32) async throws -> AmPage { artists }
        func recentlyPlayed(limit _: UInt32, offset _: UInt32) async throws -> AmPage { recents }
        func recommendations() async throws -> [AmShelf] { rails }
        func children(of uri: String, limit: UInt32, offset: UInt32) async throws -> AmPage {
            lock.withLock { childrenCalls.append((uri, limit, offset)) }
            return childrenPage
        }

        func resolve(uri _: String) async throws -> AmItem { resolved }
        func search(query _: String, limit _: UInt32) async throws -> AmSearchResults { searchResults }
        func librarySongs(limit _: UInt32, offset _: UInt32) async throws -> AmPage { songs }
        func isFavorite(uris: [String]) async throws -> [Bool] { uris.map { favoriteState[$0] ?? false } }
        func addFavorite(uri: String) async throws {
            lock.withLock {
                favoriteState[uri] = true
                favoriteWrites.append((uri, true))
            }
        }
    }

    struct Harness {
        let companion: BridgethingCompanion
        let driver: WireDriver
        let auth: FakeAuth
        let player: FakePlayer
        let library: FakeLibrary
        let glue: AppleMusicGlue
    }

    private func boot(
        auth: FakeAuth = FakeAuth(),
        player: FakePlayer = FakePlayer(),
        library: FakeLibrary = FakeLibrary(),
        autoResume: Bool = false
    ) async throws -> Harness {
        let glue = AppleMusicGlue(auth: auth, player: player, library: library)
        let adapter = InMemoryAdapter()
        let companion = BridgethingCompanion(
            adapter: adapter,
            lyricsResolver: FakeLyricsResolver(),
            host: HostInfo(appName: "applemusic-test", appVersion: "0.0.1", osName: "macOS")
        )
        try await companion.setActive(glue)
        try await companion.start()
        await companion.setDeviceAutoResume(deviceId: "carthing-test", enabled: autoResume)
        let driver = WireDriver(adapter: adapter)
        await driver.start()
        driver.connect()
        _ = try await driver.waitOutbound(timeout: .seconds(5)) {
            if case .time(.snapshot(_)) = $0.data { return true }
            return false
        }
        try await Task.sleep(for: .milliseconds(50))
        return Harness(companion: companion, driver: driver, auth: auth, player: player, library: library, glue: glue)
    }

    private func song(_ id: String, _ title: String, art: String? = nil) -> AmItem {
        AmItem(
            uri: AmUri.make(.song, id: id), kind: .song, title: title,
            artistName: "Artist", albumName: "Album", artworkUrl: art, durationMs: 200_000
        )
    }

    private func entry(_ id: String?, _ title: String, art: String? = nil) -> AmEntry {
        AmEntry(
            uri: id.map { AmUri.make(.song, id: $0) }, title: title, artistName: "Artist",
            albumName: "Album", artworkUrl: art, durationMs: 200_000
        )
    }

    // MARK: - uri + art codec

    func testAmUriRoundTrip() {
        let uri = AmUri.make(.album, id: "1440857781")
        XCTAssertEqual(uri, "applemusic:album:1440857781")
        let parsed = AmUri.parse(uri)
        XCTAssertEqual(parsed?.kind, .album)
        XCTAssertEqual(parsed?.id, "1440857781")
        XCTAssertNil(AmUri.parse("spotify:album:1"))
        XCTAssertNil(AmUri.parse("applemusic:album:"))
        XCTAssertNil(AmUri.parse("applemusic:mixtape:9"))
        XCTAssertEqual(AmUri.parse("applemusic:song:i.aBc123")?.id, "i.aBc123")
    }

    func testSizedArtworkUrlSubstitutesTemplate() {
        let template = "https://is1-ssl.mzstatic.com/image/thumb/x/{w}x{h}bb.jpg"
        XCTAssertEqual(
            sizedArtworkUrl(template, edge: 248),
            "https://is1-ssl.mzstatic.com/image/thumb/x/248x248bb.jpg"
        )
        let plain = "https://example.com/a.jpg"
        XCTAssertEqual(sizedArtworkUrl(plain, edge: 96), plain)
    }

    // MARK: - auth lifecycle

    func testDeniedAuthorizationFails() async throws {
        let auth = FakeAuth(status: .denied)
        let glue = AppleMusicGlue(auth: auth, player: FakePlayer(), library: FakeLibrary())
        let states = StateSink()
        await glue.setAuthObserver { states.append($0) }
        let adapter = InMemoryAdapter()
        let companion = BridgethingCompanion(
            adapter: adapter, lyricsResolver: FakeLyricsResolver(),
            host: HostInfo(appName: "t", appVersion: "1", osName: "macOS")
        )
        addTeardownBlock { await companion.stop() }
        try await companion.setActive(glue)
        try await companion.start()
        try await waitUntil { states.contains { if case .failed = $0 { return true }; return false } }
    }

    func testMissingSubscriptionFails() async throws {
        let auth = FakeAuth(subscribed: false)
        let glue = AppleMusicGlue(auth: auth, player: FakePlayer(), library: FakeLibrary())
        let states = StateSink()
        await glue.setAuthObserver { states.append($0) }
        let adapter = InMemoryAdapter()
        let companion = BridgethingCompanion(
            adapter: adapter, lyricsResolver: FakeLyricsResolver(),
            host: HostInfo(appName: "t", appVersion: "1", osName: "macOS")
        )
        addTeardownBlock { await companion.stop() }
        try await companion.setActive(glue)
        try await companion.start()
        try await waitUntil { states.contains { if case .failed = $0 { return true }; return false } }
    }

    func testAuthorizedEmitsAuthenticated() async throws {
        let glue = AppleMusicGlue(auth: FakeAuth(), player: FakePlayer(), library: FakeLibrary())
        let states = StateSink()
        await glue.setAuthObserver { states.append($0) }
        let adapter = InMemoryAdapter()
        let companion = BridgethingCompanion(
            adapter: adapter, lyricsResolver: FakeLyricsResolver(),
            host: HostInfo(appName: "t", appVersion: "1", osName: "macOS")
        )
        addTeardownBlock { await companion.stop() }
        try await companion.setActive(glue)
        try await companion.start()
        try await waitUntil { states.contains { if case .authenticated = $0 { return true }; return false } }
    }

    // MARK: - snapshots + authority

    func testItemClaimsAuthorityWithAppleMusicBundle() async throws {
        let player = FakePlayer()
        let h = try await boot(player: player)
        addTeardownBlock { await h.companion.stop() }

        player.push(AmPlayerSnapshot(
            entry: entry("123", "Song"), playing: true, positionMs: 5000, shuffle: false, repeatMode: .off
        ))
        let claim = try await h.driver.waitOutbound(timeout: .seconds(20)) {
            if case let .authority(.claim(c)) = $0.data, c.scope == .nowPlayingPlayback { return true }
            return false
        }
        guard case let .authority(.claim(c)) = claim.data else { return XCTFail("expected claim") }
        XCTAssertEqual(c.appBundle, "com.apple.Music")

        let snap = try await h.driver.waitOutbound(timeout: .seconds(20)) {
            if case let .player(.snapshot(ps)) = $0.data, ps.track != nil { return true }
            return false
        }
        guard case let .player(.snapshot(ps)) = snap.data else { return XCTFail("expected snapshot") }
        XCTAssertEqual(ps.track?.title, "Song")
        XCTAssertEqual(ps.track?.uri, "applemusic:song:123")
        XCTAssertEqual(ps.playback.state, .playing)
        XCTAssertEqual(ps.playback.positionMs, 5000)
    }

    func testNoItemReleasesAuthority() async throws {
        let player = FakePlayer()
        let h = try await boot(player: player)
        addTeardownBlock { await h.companion.stop() }

        player.push(AmPlayerSnapshot(entry: entry("123", "Song"), playing: true, positionMs: 0, shuffle: false, repeatMode: .off))
        _ = try await h.driver.waitOutbound(timeout: .seconds(20)) {
            if case .authority(.claim) = $0.data { return true }
            return false
        }
        player.push(AmPlayerSnapshot(entry: nil, playing: false, positionMs: 0, shuffle: false, repeatMode: .off))
        _ = try await h.driver.waitOutbound(timeout: .seconds(20)) {
            if case .authority(.release) = $0.data { return true }
            return false
        }
    }

    func testArtworkTemplateBecomesAssetId() async throws {
        let player = FakePlayer()
        let h = try await boot(player: player)
        addTeardownBlock { await h.companion.stop() }

        player.push(AmPlayerSnapshot(
            entry: entry("9", "Art Song", art: "https://is1-ssl.mzstatic.com/image/thumb/a/{w}x{h}bb.jpg"),
            playing: true, positionMs: 0, shuffle: false, repeatMode: .off
        ))
        let snap = try await h.driver.waitOutbound(timeout: .seconds(20)) {
            if case let .player(.snapshot(ps)) = $0.data, ps.track != nil { return true }
            return false
        }
        guard case let .player(.snapshot(ps)) = snap.data else { return XCTFail("expected snapshot") }
        let artworkId = try XCTUnwrap(ps.track?.artworkId)
        XCTAssertTrue(artworkId.hasPrefix("applemusic/img/248/"), "unexpected id \(artworkId)")
        XCTAssertTrue(artworkId.contains("248x248"), "edge not substituted into url: \(artworkId)")
    }

    // MARK: - transport verbs

    func testTransportVerbsMapToSeam() async throws {
        let player = FakePlayer()
        let h = try await boot(player: player)
        addTeardownBlock { await h.companion.stop() }

        try await h.glue.play(PlayUri(uri: "applemusic:song:1", context: PlayContext(contextUri: "applemusic:album:2", position: nil)))
        XCTAssertEqual(player.playContextCalls.last?.context, "applemusic:album:2")
        XCTAssertEqual(player.playContextCalls.last?.startAt, "applemusic:song:1")

        try await h.glue.play(PlayUri(uri: "applemusic:playlist:5", context: nil))
        XCTAssertEqual(player.playContextCalls.last?.context, "applemusic:playlist:5")
        XCTAssertNil(player.playContextCalls.last?.startAt)

        try await h.glue.queue(QueueUri(uri: "applemusic:song:3", position: .next))
        XCTAssertEqual(player.queueInserts.last?.next, true)
        try await h.glue.queue(QueueUri(uri: "applemusic:song:4", position: .append))
        XCTAssertEqual(player.queueInserts.last?.next, false)

        try await h.glue.pause()
        try await h.glue.resume()
        try await h.glue.skipNext()
        try await h.glue.skipPrev()
        try await h.glue.seekTo(30000)
        try await h.glue.setShuffle(true)
        try await h.glue.setRepeat(.all)
        try await h.glue.setRepeat(.one)
        XCTAssertEqual(player.pauseCount, 1)
        XCTAssertEqual(player.playCount, 1)
        XCTAssertEqual(player.nextCount, 1)
        XCTAssertEqual(player.prevCount, 1)
        XCTAssertEqual(player.seeks, [30000])
        XCTAssertEqual(player.shuffles, [true])
        XCTAssertEqual(player.repeats, [.all, .one])

        do {
            try await h.glue.skipToIndex(2)
            XCTFail("skipToIndex should be unimplemented without a readable queue")
        } catch {}
    }

    // MARK: - browse

    func testBrowseRootComposesStaplesThenRails() async throws {
        let library = FakeLibrary()
        library.playlists = AmPage(items: [AmItem(uri: "applemusic:playlist:1", kind: .playlist, title: "Mix")], total: 4, hasMore: true)
        library.rails = [AmShelf(id: "6-r", title: "Made for You", items: [song("10", "Rec")], total: 7)]
        let h = try await boot(library: library)
        addTeardownBlock { await h.companion.stop() }

        let resp = try await h.driver.request(
            .library(.browse(LibraryBrowseRequest(nodeId: nil, limit: 20, offset: 0, sections: nil, preview: nil))),
            timeout: .seconds(5)
        )
        guard case let .library(.browseReply(reply)) = resp.data else { return XCTFail("expected browseReply, got \(resp.data)") }
        let folders: [BrowseFolder] = reply.result.entries.compactMap {
            if case let .folder(f) = $0 { return f }
            return nil
        }
        XCTAssertEqual(folders.map(\.nodeId), ["playlists", "albums", "artists", "songs", "rec:6-r"])
        XCTAssertEqual(folders.first?.total, 4)
        XCTAssertEqual(folders.first?.previewChildren?.count, 1)
        XCTAssertEqual(folders.last?.title, "Made for You")
        XCTAssertEqual(folders.last?.total, 7)
    }

    func testBrowseRootSectionsCapAndIndexOnlyPreview() async throws {
        let library = FakeLibrary()
        library.rails = [AmShelf(id: "r", title: "Rail", items: [song("1", "S")], total: 1)]
        let h = try await boot(library: library)
        addTeardownBlock { await h.companion.stop() }

        let resp = try await h.driver.request(
            .library(.browse(LibraryBrowseRequest(nodeId: nil, limit: 20, offset: 0, sections: 2, preview: 0))),
            timeout: .seconds(5)
        )
        guard case let .library(.browseReply(reply)) = resp.data else { return XCTFail("expected browseReply") }
        XCTAssertEqual(reply.result.entries.count, 2)
        XCTAssertTrue(h.library.playlistCalls.isEmpty, "preview 0 must not hydrate staples")
        for entry in reply.result.entries {
            guard case let .folder(f) = entry else { return XCTFail("expected folder") }
            XCTAssertNil(f.previewChildren)
        }
    }

    func testBrowseDrilldownsRouteToSeam() async throws {
        let library = FakeLibrary()
        library.childrenPage = AmPage(items: [song("2", "Track")], total: 1, hasMore: false)
        library.recents = AmPage(items: [AmItem(uri: "applemusic:album:3", kind: .album, title: "Recent")], total: 1, hasMore: false)
        let h = try await boot(library: library)
        addTeardownBlock { await h.companion.stop() }

        let drill = try await h.driver.request(
            .library(.browse(LibraryBrowseRequest(nodeId: "applemusic:album:7", limit: 10, offset: 5, sections: nil, preview: nil))),
            timeout: .seconds(5)
        )
        guard case .library(.browseReply) = drill.data else { return XCTFail("expected browseReply") }
        XCTAssertEqual(h.library.childrenCalls.last?.uri, "applemusic:album:7")
        XCTAssertEqual(h.library.childrenCalls.last?.limit, 10)
        XCTAssertEqual(h.library.childrenCalls.last?.offset, 5)

        let recents = try await h.driver.request(
            .library(.browse(LibraryBrowseRequest(nodeId: "recently-played", limit: 10, offset: 0, sections: nil, preview: nil))),
            timeout: .seconds(5)
        )
        guard case let .library(.browseReply(reply)) = recents.data else { return XCTFail("expected browseReply") }
        guard case let .item(.album(album)) = reply.result.entries.first else { return XCTFail("expected album item") }
        XCTAssertEqual(album.name, "Recent")
    }

    // MARK: - search + favorites

    func testSearchMapsKinds() async throws {
        let library = FakeLibrary()
        library.searchResults = AmSearchResults(
            songs: [song("1", "S")],
            albums: [AmItem(uri: "applemusic:album:2", kind: .album, title: "A")],
            artists: [AmItem(uri: "applemusic:artist:3", kind: .artist, title: "Ar")],
            playlists: []
        )
        let h = try await boot(library: library)
        addTeardownBlock { await h.companion.stop() }

        let resp = try await h.driver.request(
            .library(.search(LibrarySearchRequest(query: "q", kinds: [.track, .album], limit: 10, offset: 0))),
            timeout: .seconds(5)
        )
        guard case let .library(.searchReply(reply)) = resp.data else { return XCTFail("expected searchReply, got \(resp.data)") }
        XCTAssertEqual(reply.result.items.count, 2)
        XCTAssertEqual(reply.result.kinds, [.track, .album])
    }

    func testFavoritesToggleWritesAndReemitsLiked() async throws {
        let player = FakePlayer()
        let library = FakeLibrary()
        let h = try await boot(player: player, library: library)
        addTeardownBlock { await h.companion.stop() }

        player.push(AmPlayerSnapshot(entry: entry("42", "Fav Song"), playing: true, positionMs: 0, shuffle: false, repeatMode: .off))
        _ = try await h.driver.waitOutbound(timeout: .seconds(20)) {
            if case .player(.snapshot) = $0.data { return true }
            return false
        }

        try await h.driver.send(
            .library(.favoritesToggle(FavoritesToggle(item: ItemRef(uri: "applemusic:song:42", kind: .track, persistentId: nil))))
        )
        let reemit = try await h.driver.waitOutbound(timeout: .seconds(20)) {
            if case let .player(.snapshot(ps)) = $0.data, ps.track?.liked == true { return true }
            return false
        }
        guard case let .player(.snapshot(ps)) = reemit.data else { return XCTFail("expected re-emit") }
        XCTAssertEqual(ps.track?.liked, true)
        XCTAssertEqual(h.library.favoriteWrites.last?.0, "applemusic:song:42")
        XCTAssertEqual(h.library.favoriteWrites.last?.1, true)
    }

    func testUnfavoriteIsRefusedAddOnly() async throws {
        let library = FakeLibrary()
        library.favoriteState["applemusic:song:9"] = true
        let h = try await boot(library: library)
        addTeardownBlock { await h.companion.stop() }

        do {
            try await h.glue.favoritesSet(ItemRef(uri: "applemusic:song:9", kind: .track, persistentId: nil), liked: false)
            XCTFail("unfavorite must be refused; apple's favorites api is add-only")
        } catch {}
        do {
            try await h.glue.favoritesToggle(ItemRef(uri: "applemusic:song:9", kind: .track, persistentId: nil))
            XCTFail("toggling an already-favorited song must be refused, not silently dropped")
        } catch {}
        XCTAssertTrue(h.library.favoriteWrites.isEmpty, "no writes may reach the seam for unfavorite paths")
    }

    // MARK: - resume on connect

    func testPeerConnectResumesWhenIdleAndQuiet() async throws {
        let player = FakePlayer()
        _ = try await bootResume(player: player, autoResume: true)
        try await waitUntil { player.playCount == 1 }
    }

    func testPeerConnectStandsDownForOtherAudio() async throws {
        let player = FakePlayer()
        player.otherAudio = true
        let h = try await bootResume(player: player, autoResume: true)
        try await Task.sleep(for: .milliseconds(300))
        XCTAssertEqual(player.playCount, 0)
        _ = h
    }

    func testPeerConnectRespectsAutoResumeOff() async throws {
        let player = FakePlayer()
        let h = try await bootResume(player: player, autoResume: false)
        try await Task.sleep(for: .milliseconds(300))
        XCTAssertEqual(player.playCount, 0)
        _ = h
    }

    private func bootResume(player: FakePlayer, autoResume: Bool) async throws -> Harness {
        let glue = AppleMusicGlue(auth: FakeAuth(), player: player, library: FakeLibrary())
        let states = StateSink()
        await glue.setAuthObserver { states.append($0) }
        let adapter = InMemoryAdapter()
        let companion = BridgethingCompanion(
            adapter: adapter, lyricsResolver: FakeLyricsResolver(),
            host: HostInfo(appName: "t", appVersion: "1", osName: "macOS")
        )
        addTeardownBlock { await companion.stop() }
        try await companion.setActive(glue)
        try await companion.start()
        try await waitUntil { states.contains { if case .authenticated = $0 { return true }; return false } }
        await companion.setDeviceAutoResume(deviceId: "carthing-test", enabled: autoResume)
        let driver = WireDriver(adapter: adapter)
        await driver.start()
        driver.connect()
        _ = try await driver.waitOutbound(timeout: .seconds(5)) {
            if case .time(.snapshot(_)) = $0.data { return true }
            return false
        }
        return Harness(companion: companion, driver: driver, auth: FakeAuth(), player: player, library: FakeLibrary(), glue: glue)
    }

    // MARK: - helpers

    final class StateSink: @unchecked Sendable {
        private let lock = NSLock()
        private var states: [GlueAuthState] = []
        func append(_ s: GlueAuthState) { lock.withLock { states.append(s) } }
        func contains(_ pred: (GlueAuthState) -> Bool) -> Bool { lock.withLock { states.contains(where: pred) } }
    }

    private func waitUntil(
        timeout: Duration = .seconds(10), _ predicate: @escaping () -> Bool
    ) async throws {
        let deadline = ContinuousClock.now + timeout
        while ContinuousClock.now < deadline {
            if predicate() { return }
            try await Task.sleep(for: .milliseconds(25))
        }
        XCTFail("condition not met within \(timeout)")
    }
}
