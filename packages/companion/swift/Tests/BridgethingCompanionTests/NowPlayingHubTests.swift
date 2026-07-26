import BridgethingGateway
import BridgethingGlue
import BridgethingSchema
import BridgethingTestKit
import XCTest

@testable import BridgethingCompanion

private func makeSnapshot(state: PlaybackState, uri: String) -> PlayerState {
    PlayerState(
        track: MediaItem(
            uri: uri, persistentId: nil, title: "t", album: nil, albumUri: nil, albumArtist: nil,
            artist: nil, artistUri: nil, liked: nil, artworkId: nil, durationMs: nil, mediaTypes: nil,
            trackNumber: nil, trackCount: nil, isLikeSupported: nil, isBanSupported: nil,
            isBanned: nil, chapterCount: nil
        ),
        playback: Playback(
            state: state, positionMs: 0, positionAgeMs: nil, shuffle: false, shuffleMode: nil,
            repeat: .off, queueIndex: nil, queueCount: nil, queueChapterIndex: nil,
            setElapsedTimeAvailable: nil, queueListAvail: nil, appleMusicRadioAd: nil
        ),
        queue: [],
        options: PlayerOptions(speed: 1, crossfadeMs: nil),
        context: nil,
        target: nil
    )
}

final class NowPlayingHubTests: XCTestCase {
    private func makeHub() async throws -> (NowPlayingHub, WireDriver) {
        let adapter = InMemoryAdapter()
        let gateway = BridgethingGateway(adapter: adapter)
        try await gateway.start()
        let driver = WireDriver(adapter: adapter)
        await driver.start()
        driver.connect()
        let hub = NowPlayingHub(gateway: gateway)
        hub.start()
        return (hub, driver)
    }

    func testPlayingSourceWinsOverPausedOne() async throws {
        let (hub, driver) = try await makeHub()
        hub.submitPlayer(
            sourceId: "spotify", snapshot: makeSnapshot(state: .paused, uri: "spotify:track:a"),
            appBundle: "com.spotify.client", hasItem: true, wantsVolume: false
        )
        hub.submitPlayer(
            sourceId: "applemusic", snapshot: makeSnapshot(state: .playing, uri: "applemusic:song:b"),
            appBundle: "com.apple.Music", hasItem: true, wantsVolume: false
        )
        try await Task.sleep(nanoseconds: 100_000_000)
        XCTAssertEqual(hub.currentSource(), "applemusic")
        _ = driver
    }

    func testMostRecentWinsWhenNothingPlaying() async throws {
        let (hub, _) = try await makeHub()
        hub.submitPlayer(
            sourceId: "applemusic", snapshot: makeSnapshot(state: .paused, uri: "applemusic:song:b"),
            appBundle: "com.apple.Music", hasItem: true, wantsVolume: false
        )
        try await Task.sleep(nanoseconds: 50_000_000)
        hub.submitPlayer(
            sourceId: "spotify", snapshot: makeSnapshot(state: .paused, uri: "spotify:track:a"),
            appBundle: "com.spotify.client", hasItem: true, wantsVolume: false
        )
        try await Task.sleep(nanoseconds: 100_000_000)
        XCTAssertEqual(hub.currentSource(), "spotify")
    }

    func testPlayingSourceTakesOverFromPlayingOne() async throws {
        let (hub, _) = try await makeHub()
        hub.submitPlayer(
            sourceId: "spotify", snapshot: makeSnapshot(state: .playing, uri: "spotify:track:a"),
            appBundle: "com.spotify.client", hasItem: true, wantsVolume: false
        )
        try await Task.sleep(nanoseconds: 50_000_000)
        hub.submitPlayer(
            sourceId: "applemusic", snapshot: makeSnapshot(state: .playing, uri: "applemusic:song:b"),
            appBundle: "com.apple.Music", hasItem: true, wantsVolume: false
        )
        try await Task.sleep(nanoseconds: 100_000_000)
        XCTAssertEqual(hub.currentSource(), "applemusic")
    }

    func testClearingCurrentSourceFallsBackToTheOther() async throws {
        let (hub, _) = try await makeHub()
        hub.submitPlayer(
            sourceId: "spotify", snapshot: makeSnapshot(state: .paused, uri: "spotify:track:a"),
            appBundle: "com.spotify.client", hasItem: true, wantsVolume: false
        )
        hub.submitPlayer(
            sourceId: "applemusic", snapshot: makeSnapshot(state: .playing, uri: "applemusic:song:b"),
            appBundle: "com.apple.Music", hasItem: true, wantsVolume: false
        )
        try await Task.sleep(nanoseconds: 100_000_000)
        hub.clearSource(sourceId: "applemusic")
        try await Task.sleep(nanoseconds: 100_000_000)
        XCTAssertEqual(hub.currentSource(), "spotify")
    }

    func testTransportRoutesToTheAudibleSource() async throws {
        let (hub, _) = try await makeHub()
        let spotify = FakeGlue(uriSchemes: ["spotify"])
        let apple = OtherFakeGlue()
        hub.register(sourceId: "spotify", transport: spotify)
        hub.register(sourceId: "applemusic", transport: apple)

        hub.submitPlayer(
            sourceId: "spotify", snapshot: makeSnapshot(state: .paused, uri: "spotify:track:a"),
            appBundle: "com.spotify.client", hasItem: true, wantsVolume: false
        )
        hub.submitPlayer(
            sourceId: "applemusic", snapshot: makeSnapshot(state: .playing, uri: "applemusic:song:b"),
            appBundle: "com.apple.Music", hasItem: true, wantsVolume: false
        )
        try await Task.sleep(nanoseconds: 100_000_000)

        try await hub.currentTransport()?.pause()
        let applePaused = await apple.paused
        XCTAssertTrue(applePaused)
        XCTAssertFalse(spotify.calls.contains(.pause))
    }

    func testVolumeScopeIsClaimedOnlyWhenTheAudibleSourceWantsIt() async throws {
        let (hub, driver) = try await makeHub()
        hub.submitPlayer(
            sourceId: "spotify", snapshot: makeSnapshot(state: .playing, uri: "spotify:track:a"),
            appBundle: "com.spotify.client", hasItem: true, wantsVolume: true
        )
        let claimed = try await driver.waitOutbound { msg in
            guard case let .authority(auth) = msg.data, case let .claim(c) = auth else { return false }
            return c.scope == .volume
        }
        XCTAssertNotNil(claimed)
    }

    func testVolumeScopeIsReleasedWhenTheSourceLeavesTheRemoteSpeaker() async throws {
        let (hub, driver) = try await makeHub()
        hub.submitPlayer(
            sourceId: "spotify", snapshot: makeSnapshot(state: .playing, uri: "spotify:track:a"),
            appBundle: "com.spotify.client", hasItem: true, wantsVolume: true
        )
        _ = try await driver.waitOutbound { msg in
            guard case let .authority(auth) = msg.data, case let .claim(c) = auth else { return false }
            return c.scope == .volume
        }
        hub.submitPlayer(
            sourceId: "spotify", snapshot: makeSnapshot(state: .playing, uri: "spotify:track:a"),
            appBundle: "com.spotify.client", hasItem: true, wantsVolume: false
        )
        let released = try await driver.waitOutbound { msg in
            guard case let .authority(auth) = msg.data, case let .release(r) = auth else { return false }
            return r.scope == .volume
        }
        XCTAssertNotNil(released)
    }
}

private actor OtherFakeGlue: BridgethingGlue {
    static let name = "applemusic"
    static let displayName = "Other"

    nonisolated var capabilities: GlueCapabilities { [] }
    nonisolated var uriSchemes: [String] { ["applemusic"] }
    nonisolated var musicProvider: MusicProvider { .appleMusic }
    nonisolated var lyricsSupported: Bool { false }

    private(set) var paused = false

    func attach(gateway _: BridgethingGateway) async throws {}
    func detach() async {}
    func pause() async throws { paused = true }
}
