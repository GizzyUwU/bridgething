import BridgethingCompanion
import BridgethingGateway
import BridgethingGlue
import BridgethingSchema
import BridgethingTestKit
import Foundation
import Spotify
import XCTest

@testable import BridgethingSpotifyGlue

/// drives the glue over a fake first-party client + the in-memory wire, asserting the
/// reduced-delta -> gateway-wire mapping, authority claim/release (incl. the cast gate),
/// and the queue push. no network; the dealer firehose is injected via the Observer.
final class SpotifyGlueTests: XCTestCase {
    struct Harness {
        let companion: BridgethingCompanion
        let driver: WireDriver
        let fake: FakeClient
        let glue: SpotifyGlue
    }

    final class FakeTokenStore: Spotify.TokenStore, @unchecked Sendable {
        var refresh: String?
        var username: String?
        init(refresh: String?) { self.refresh = refresh }
        func loadRefreshToken() -> String? { refresh }
        func saveRefreshToken(token: String) { refresh = token }
        func loadUsername() -> String? { username }
        func saveUsername(username: String) { self.username = username }
    }

    final class FakeClient: SpotifyClientProviding, @unchecked Sendable {
        var observer: (any Spotify.Observer)?
        var root: [Spotify.Shelf] = []
        var page = Spotify.BrowsePage(items: [], total: 0, hasMore: false)
        var searchResults = Spotify.SearchResults(tracks: [], albums: [], artists: [], playlists: [])
        var contains: [Bool] = []
        var productState = Spotify.ProductState(product: "premium", catalogue: "premium", country: "US", isPremium: true, canUseSuperbird: true)
        var likedWrites: [(String, Bool)] = []
        var playCalls: [(uri: String, skipToUri: String?)] = []
        var currentPosition: UInt32?
        var volume: Double = 50
        var volumeSets: [Double] = []

        func connect() async throws {}
        func disconnect() async {}
        func resync() async {}
        func currentPositionMs() async -> UInt32? { currentPosition }
        func pause() async throws {}
        func resume() async throws {}
        func skipNext() async throws {}
        func skipPrev() async throws {}
        func seek(positionMs _: Int64) async throws {}
        func setShuffle(on _: Bool) async throws {}
        func setRepeat(mode _: Spotify.RepeatMode) async throws {}
        func queueUri(uri _: String) async throws {}
        func play(uri: String, skipToUri: String?) async throws { playCalls.append((uri, skipToUri)) }
        func setVolume(percent: Double) async throws {
            volume = percent
            volumeSets.append(percent)
        }
        func volumeStep(deltaPercent: Double) async throws -> Double {
            volume = min(100, max(0, volume + deltaPercent))
            volumeSets.append(volume)
            return volume
        }
        func activeDeviceVolumePercent() async -> Double? { volume }
        func product() async throws -> Spotify.ProductState { productState }
        func rootBrowse() async throws -> [Spotify.Shelf] { root }
        func browse(nodeId _: String, limit _: UInt32, offset _: UInt32) async throws -> Spotify.BrowsePage { page }
        func search(query _: String, limit _: UInt32) async throws -> Spotify.SearchResults { searchResults }
        func resolveContext(uri _: String) async throws -> Spotify.BrowseItem { item("spotify:playlist:1", "Ctx") }
        func favoritesContains(uris _: [String]) async throws -> [Bool] { contains }
        func favoritesSet(uri: String, liked: Bool) async throws { likedWrites.append((uri, liked)) }
        func favoritesList(limit _: UInt32, offset _: UInt32) async throws -> Spotify.BrowsePage { page }
    }

    private func boot(_ fake: FakeClient, paired: Bool = true) async throws -> Harness {
        let glue = SpotifyGlue(
            workerBase: "https://example/auth",
            psk: "psk",
            deviceId: "dev",
            tokenStore: FakeTokenStore(refresh: paired ? "rt" : nil),
            clientFactory: { _, observer in fake.observer = observer; return fake }
        )
        let adapter = InMemoryAdapter()
        let companion = BridgethingCompanion(
            adapter: adapter,
            lyricsResolver: FakeLyricsResolver(),
            host: HostInfo(appName: "spotify-test", appVersion: "0.0.1", osName: "macOS")
        )
        try await companion.setActive(glue)
        try await companion.start()
        let driver = WireDriver(adapter: adapter)
        await driver.start()
        driver.connect()
        // barrier: the peer-connected handler resets glue authority out of band of the
        // emit queue; the time snapshot is its last frame, so wait it out before tests
        // drive dealer events or claims race the reset
        _ = try await driver.waitOutbound(timeout: .seconds(5)) {
            if case .time(.snapshot(_)) = $0.data { return true }
            return false
        }
        try await Task.sleep(for: .milliseconds(50))
        return Harness(companion: companion, driver: driver, fake: fake, glue: glue)
    }

    // MARK: - library mapping

    func testBrowseRootMapsShelvesToFolders() async throws {
        let fake = FakeClient()
        fake.root = [
            Spotify.Shelf(id: "playlists", title: "Playlists", items: [item("spotify:playlist:1", "Mix", hasChildren: true)]),
            Spotify.Shelf(id: "albums", title: "Albums", items: [item("spotify:album:1", "Album")]),
        ]
        let h = try await boot(fake)
        addTeardownBlock { await h.companion.stop() }
        let resp = try await h.driver.request(.library(.browse(LibraryBrowseRequest(nodeId: nil, limit: 20, offset: 0))), timeout: .seconds(5))
        guard case let .library(.browseReply(reply)) = resp.data else { return XCTFail("expected browseReply, got \(resp.data)") }
        XCTAssertEqual(reply.result.entries.count, 2)
        guard case let .folder(folder) = reply.result.entries.first else { return XCTFail("expected folder") }
        XCTAssertEqual(folder.nodeId, "playlists")
        XCTAssertEqual(folder.title, "Playlists")
        XCTAssertEqual(folder.previewChildren?.count, 1)
    }

    func testBrowseDrillInMapsItemsByKind() async throws {
        let fake = FakeClient()
        fake.page = Spotify.BrowsePage(
            items: [track("spotify:track:1", "Song"), item("spotify:album:9", "Alb")],
            total: 2, hasMore: false
        )
        let h = try await boot(fake)
        addTeardownBlock { await h.companion.stop() }
        let resp = try await h.driver.request(.library(.browse(LibraryBrowseRequest(nodeId: "albums", limit: 20, offset: 0))), timeout: .seconds(5))
        guard case let .library(.browseReply(reply)) = resp.data else { return XCTFail("expected browseReply") }
        XCTAssertEqual(reply.result.entries.count, 2)
        guard case let .item(.track(t)) = reply.result.entries.first else { return XCTFail("expected a track item") }
        XCTAssertEqual(t.name, "Song")
        XCTAssertFalse(t.image_id.isEmpty, "track art id should be wrapped")
    }

    func testSearchMapsByRequestedKinds() async throws {
        let fake = FakeClient()
        fake.searchResults = Spotify.SearchResults(
            tracks: [track("spotify:track:1", "T")],
            albums: [item("spotify:album:1", "A")],
            artists: [], playlists: []
        )
        let h = try await boot(fake)
        addTeardownBlock { await h.companion.stop() }
        let resp = try await h.driver.request(.library(.search(LibrarySearchRequest(query: "x", kinds: [.track, .album], limit: 10, offset: 0))), timeout: .seconds(5))
        guard case let .library(.searchReply(reply)) = resp.data else { return XCTFail("expected searchReply") }
        XCTAssertEqual(reply.result.items.count, 2)
        XCTAssertEqual(reply.result.kinds, [.track, .album])
    }

    // MARK: - now-playing + authority

    func testPlayerPushSnapshotsAndClaimsAuthority() async throws {
        let fake = FakeClient()
        let h = try await boot(fake)
        addTeardownBlock { await h.companion.stop() }
        fake.observer?.onPlayer(state: state(npTrack("spotify:track:1", "Song")))

        let snap = try await h.driver.waitOutbound(timeout: .seconds(20)) { if case .player(.snapshot) = $0.data { return true }; return false }
        guard case let .player(.snapshot(ps)) = snap.data else { return XCTFail("expected snapshot") }
        XCTAssertEqual(ps.track?.title, "Song")
        XCTAssertEqual(ps.playback.state, .playing)

        let claim = try await h.driver.waitOutbound(timeout: .seconds(20)) {
            if case let .authority(.claim(c)) = $0.data, c.scope == .nowPlayingPlayback { return true }
            return false
        }
        guard case let .authority(.claim(c)) = claim.data else { return XCTFail("expected claim") }
        XCTAssertEqual(c.appBundle, "com.spotify.client")
    }

    func testPeerReconnectReplaysFreshPositionNotStaleZero() async throws {
        let fake = FakeClient()
        fake.currentPosition = 90_000
        let h = try await boot(fake)
        addTeardownBlock { await h.companion.stop() }
        fake.observer?.onPlayer(state: state(npTrack("spotify:track:1", "Song")))
        let first = try await h.driver.waitOutbound(timeout: .seconds(20)) { if case .player(.snapshot) = $0.data { return true }; return false }
        guard case let .player(.snapshot(stale)) = first.data else { return XCTFail("expected snapshot") }
        XCTAssertEqual(stale.playback.positionMs, 0, "the cached now-playing position is frozen at the last dealer event")

        await h.glue.handlePeerConnected()
        let replay = try await h.driver.waitOutbound(timeout: .seconds(20)) {
            if case let .player(.snapshot(ps)) = $0.data, ps.playback.positionMs == 90_000 { return true }
            return false
        }
        guard case let .player(.snapshot(ps)) = replay.data else { return XCTFail("expected replay snapshot") }
        XCTAssertEqual(ps.playback.positionMs, 90_000, "peer-connect replay must refresh the stale cached position")
    }

    func testRemoteConnectPlaybackClaimsAuthorityAndVolume() async throws {
        let fake = FakeClient()
        let h = try await boot(fake)
        addTeardownBlock { await h.companion.stop() }
        fake.observer?.onPlayer(state: state(npTrack("spotify:track:1", "Song"), remote: true))

        for scope in [CompanionAuthorityScope.nowPlayingPlayback, .nowPlayingMetadata, .volume] {
            _ = try await h.driver.waitOutbound(timeout: .seconds(20)) {
                if case let .authority(.claim(c)) = $0.data, c.scope == scope { return true }
                return false
            }
        }
        let volumeState = try await h.driver.waitOutbound(timeout: .seconds(20)) {
            if case .audio(.volumeChanged) = $0.data { return true }
            return false
        }
        guard case let .audio(.volumeChanged(v)) = volumeState.data else { return XCTFail("expected volumeChanged") }
        XCTAssertEqual(v.level, 0.5, accuracy: 0.001, "volume claim must seed the remote device's cluster volume")
    }


    func testReturnToLocalPlaybackReleasesVolumeAuthority() async throws {
        let fake = FakeClient()
        let h = try await boot(fake)
        addTeardownBlock { await h.companion.stop() }
        fake.observer?.onPlayer(state: state(npTrack("spotify:track:1", "Song"), remote: true))
        _ = try await h.driver.waitOutbound(timeout: .seconds(20)) {
            if case let .authority(.claim(c)) = $0.data, c.scope == .volume { return true }
            return false
        }

        fake.observer?.onPlayer(state: state(npTrack("spotify:track:1", "Song"), remote: false))
        let release = try await h.driver.waitOutbound(timeout: .seconds(20)) {
            if case let .authority(.release(r)) = $0.data, r.scope == .volume { return true }
            return false
        }
        guard case .authority(.release) = release.data else { return XCTFail("expected volume release") }
    }

    func testVolumeVerbsRouteToRemoteDeviceWhileRemote() async throws {
        let fake = FakeClient()
        let h = try await boot(fake)
        addTeardownBlock { await h.companion.stop() }
        fake.observer?.onPlayer(state: state(npTrack("spotify:track:1", "Song"), remote: true))
        _ = try await h.driver.waitOutbound(timeout: .seconds(20)) {
            if case let .authority(.claim(c)) = $0.data, c.scope == .volume { return true }
            return false
        }

        try await h.driver.send(.audio(.volumeUp))
        let bumped = try await h.driver.waitOutbound(timeout: .seconds(20)) {
            if case let .audio(.volumeChanged(v)) = $0.data, v.level > 0.55 { return true }
            return false
        }
        guard case let .audio(.volumeChanged(v)) = bumped.data else { return XCTFail("expected volumeChanged") }
        XCTAssertEqual(v.level, 0.5625, accuracy: 0.001, "volumeUp must step the remote connect device")
        XCTAssertEqual(fake.volumeSets.last ?? 0, 56.25, accuracy: 0.01)
    }

    func testQueuePushSendsQueueChanged() async throws {
        let fake = FakeClient()
        let h = try await boot(fake)
        addTeardownBlock { await h.companion.stop() }
        fake.observer?.onQueue(queue: Spotify.Queue(previous: [], current: nil, next: [npTrack("spotify:track:2", "Next")]))

        let q = try await h.driver.waitOutbound(timeout: .seconds(20)) { if case .player(.queueChanged) = $0.data { return true }; return false }
        guard case let .player(.queueChanged(snap)) = q.data else { return XCTFail("expected queueChanged") }
        XCTAssertEqual(snap.order, ["spotify:track:2"])
        XCTAssertEqual(snap.items.first?.title, "Next")
    }

    func testLibraryChangeRelaysToGateway() async throws {
        let fake = FakeClient()
        let h = try await boot(fake)
        addTeardownBlock { await h.companion.stop() }
        fake.observer?.onLibraryChanged(scope: .playlists)

        let ev = try await h.driver.waitOutbound(timeout: .seconds(20)) {
            if case .library(.libraryChanged) = $0.data { return true }; return false
        }
        guard case let .library(.libraryChanged(changed)) = ev.data else { return XCTFail("expected libraryChanged") }
        XCTAssertEqual(changed.scope, .playlists)
    }

    func testSkipToIndexPlaysContextSkippingToQueueUri() async throws {
        let fake = FakeClient()
        let h = try await boot(fake)
        addTeardownBlock { await h.companion.stop() }
        // the player push seeds the context uri; the queue push seeds the upcoming items.
        fake.observer?.onPlayer(state: state(npTrack("spotify:track:1", "Now")))
        _ = try await h.driver.waitOutbound(timeout: .seconds(20)) { if case .player(.snapshot) = $0.data { return true }; return false }
        fake.observer?.onQueue(queue: Spotify.Queue(
            previous: [], current: nil,
            next: [npTrack("spotify:track:2", "Up1"), npTrack("spotify:track:3", "Up2")]
        ))
        _ = try await h.driver.waitOutbound(timeout: .seconds(20)) { if case .player(.queueChanged) = $0.data { return true }; return false }

        try await h.glue.skipToIndex(1)
        XCTAssertEqual(fake.playCalls.count, 1)
        XCTAssertEqual(fake.playCalls.first?.uri, "spotify:playlist:1")
        XCTAssertEqual(fake.playCalls.first?.skipToUri, "spotify:track:3")
    }

    func testSkipToIndexOutOfRangeThrowsAndDoesNotPlay() async throws {
        let fake = FakeClient()
        let h = try await boot(fake)
        addTeardownBlock { await h.companion.stop() }
        fake.observer?.onPlayer(state: state(npTrack("spotify:track:1", "Now")))
        _ = try await h.driver.waitOutbound(timeout: .seconds(20)) { if case .player(.snapshot) = $0.data { return true }; return false }
        fake.observer?.onQueue(queue: Spotify.Queue(previous: [], current: nil, next: [npTrack("spotify:track:2", "Up1")]))
        _ = try await h.driver.waitOutbound(timeout: .seconds(20)) { if case .player(.queueChanged) = $0.data { return true }; return false }

        do {
            try await h.glue.skipToIndex(5)
            XCTFail("expected an out-of-range error")
        } catch {}
        XCTAssertTrue(fake.playCalls.isEmpty, "an out-of-range index must not issue a play")
    }

    // MARK: - auth mapping + liked

    func testPremiumGateEmitsFailed() async throws {
        let fake = FakeClient()
        fake.productState = Spotify.ProductState(product: "free", catalogue: "free", country: "US", isPremium: false, canUseSuperbird: false)
        let h = try await boot(fake)
        addTeardownBlock { await h.companion.stop() }
        let failed = expectation(description: "premium gate failed auth")
        await h.glue.setAuthObserver { st in if case .failed = st { failed.fulfill() } }
        fake.observer?.onAuth(state: .loggedIn(username: "u"))
        await fulfillment(of: [failed], timeout: 5)
    }

    func testDeviceFlowPendingSurfacesPrompt() async throws {
        let fake = FakeClient()
        let h = try await boot(fake, paired: false)
        addTeardownBlock { await h.companion.stop() }
        let pending = expectation(description: "pending prompt")
        await h.glue.setAuthObserver { st in
            if case let .pending(prompt) = st, prompt?.userCode == "XYZ9" { pending.fulfill() }
        }
        fake.observer?.onAuth(state: .pending(url: "https://spotify.com/pair", code: "XYZ9"))
        await fulfillment(of: [pending], timeout: 5)
    }

    func testLoggedOutClearsNowPlayingAndReleasesAuthority() async throws {
        let fake = FakeClient()
        let h = try await boot(fake)
        addTeardownBlock { await h.companion.stop() }

        // a playing track claims authority + sets now-playing.
        fake.observer?.onPlayer(state: state(npTrack("spotify:track:1", "Song")))
        _ = try await h.driver.waitOutbound(timeout: .seconds(20)) {
            if case let .authority(.claim(c)) = $0.data, c.scope == .nowPlayingPlayback { return true }
            return false
        }
        let cleared = expectation(description: "now-playing cleared on logout")
        // the deferred companion.stop() -> detach clears now-playing a second time
        cleared.assertForOverFulfill = false
        await h.glue.setNowPlayingObserver { np in if np == nil { cleared.fulfill() } }

        fake.observer?.onAuth(state: .loggedOut)

        await fulfillment(of: [cleared], timeout: 5)
        let release = try await h.driver.waitOutbound(timeout: .seconds(20)) {
            if case .authority(.release) = $0.data { return true }
            return false
        }
        guard case .authority(.release) = release.data else { return XCTFail("expected an authority release on logout") }
        let dbg = await h.glue.debugState()
        XCTAssertFalse(dbg.authorityPlaybackHeld, "logout must release playback authority")
    }

    func testNowPlayingLikedComesFromRustSaved() async throws {
        let fake = FakeClient()
        let h = try await boot(fake)
        addTeardownBlock { await h.companion.stop() }
        fake.observer?.onPlayer(state: state(npTrack("spotify:track:1", "Song", saved: true)))
        let snap = try await h.driver.waitOutbound(timeout: .seconds(20)) { if case .player(.snapshot) = $0.data { return true }; return false }
        guard case let .player(.snapshot(ps)) = snap.data else { return XCTFail("expected snapshot") }
        XCTAssertEqual(ps.track?.liked, true, "liked must come from rust-provided saved, no favoritesContains round-trip")
        XCTAssertTrue(fake.contains.isEmpty, "no per-track favoritesContains call on the now-playing path")
    }
}

// MARK: - reduced-model builders

private func item(_ uri: String, _ title: String, hasChildren: Bool = false) -> Spotify.BrowseItem {
    Spotify.BrowseItem(
        uri: uri, title: title, subtitle: "", imageId: "ab67616d00001e02deadbeef",
        artists: [], album: Spotify.Album(uri: "", name: "", imageId: ""),
        durationMs: 0, saved: false, playable: true, hasChildren: hasChildren
    )
}

private func track(_ uri: String, _ name: String) -> Spotify.BrowseItem {
    Spotify.BrowseItem(
        uri: uri, title: name, subtitle: "Artist", imageId: "ab67616d00001e02deadbeef",
        artists: [Spotify.Artist(uri: "spotify:artist:1", name: "Artist")],
        album: Spotify.Album(uri: "spotify:album:1", name: "Album", imageId: "ab67616d00001e02deadbeef"),
        durationMs: 1000, saved: false, playable: true, hasChildren: false
    )
}

private func npTrack(_ uri: String, _ name: String, saved: Bool = false) -> Spotify.Track {
    Spotify.Track(
        uri: uri, uid: "", name: name,
        artists: [Spotify.Artist(uri: "spotify:artist:1", name: "Artist")],
        album: Spotify.Album(uri: "spotify:album:1", name: "Album", imageId: "ab67616d00001e02deadbeef"),
        durationMs: 1000, imageId: "ab67616d00001e02deadbeef", isEpisode: false, saved: saved
    )
}

private func state(_ t: Spotify.Track, remote: Bool = false) -> Spotify.PlayerState {
    Spotify.PlayerState(
        track: t, contextUri: "spotify:playlist:1", contextName: "Ctx", isPaused: false,
        positionMs: 0, durationMs: t.durationMs, shuffle: false, repeat: .off,
        playingRemotely: remote, remoteDeviceId: remote ? "speaker" : "", onRemoteSpeaker: remote,
        canSeek: true, canSkipNext: true, canSkipPrev: true, canToggleShuffle: true,
        canRepeatContext: true, canRepeatTrack: true
    )
}
