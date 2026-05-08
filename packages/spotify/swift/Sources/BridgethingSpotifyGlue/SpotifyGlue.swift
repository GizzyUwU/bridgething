import BridgethingGateway
import BridgethingGlue
import BridgethingLyrics
import BridgethingSchema
import Foundation
import Spotiny
#if canImport(FoundationNetworking)
    import FoundationNetworking
#endif

public typealias WireRepeat = BridgethingSchema.RepeatMode
private typealias SpotinyRepeat = Spotiny.RepeatMode

private let assetIdPrefix = "spotify/img/"
private let hintDebounceNanos: UInt64 = 250_000_000
private let pollIntervalNanos: UInt64 = 60_000_000_000
private let spotifyAppBundle = "com.spotify.client"

public final class SpotifyGlue: BridgethingGlue, @unchecked Sendable {
    public static let name: String = "spotify"
    public static let displayName: String = "Spotify"

    public let capabilities: GlueCapabilities = [
        .streaming,
        .queue,
        .albumArt,
        .recommendations,
        .recentlyPlayed,
        .library,
        .playlists,
    ]

    public let uriSchemes: [String] = ["spotify"]
    public let musicProvider: MusicProvider = .spotify
    public let lyricsSupported: Bool = true

    public typealias TokenCallback = @Sendable (_ accessToken: String, _ refreshToken: String) -> Void

    private let authenticator: any OAuthAuthenticator
    private let initialAccessToken: String
    private let initialRefreshToken: String
    private let onTokensRefreshed: TokenCallback?
    private let urlSession: URLSession

    private var client: SpotinyClient?
    private var gateway: BridgethingGateway?
    private var authorityHeld: Bool = false
    private var nowPlayingObserver: (@Sendable (GlueNowPlaying?) -> Void)?
    private var hintFetchTask: Task<Void, Never>?
    private var baselinePollTask: Task<Void, Never>?

    public init(
        authenticator: any OAuthAuthenticator,
        accessToken: String = "",
        refreshToken: String = "",
        onTokensRefreshed: TokenCallback? = nil,
        urlSession: URLSession = .shared
    ) {
        self.authenticator = authenticator
        initialAccessToken = accessToken
        initialRefreshToken = refreshToken
        self.onTokensRefreshed = onTokensRefreshed
        self.urlSession = urlSession
    }

    public func attach(gateway: BridgethingGateway) async throws {
        if self.gateway != nil { throw GlueError.notImplemented }

        self.gateway = gateway

        let client = SpotinyClient(
            authenticator: authenticator,
            accessToken: initialAccessToken,
            refreshToken: initialRefreshToken,
            delegate: self
        )
        self.client = client

        await client.connect()
    }

    public func detach() async {
        hintFetchTask?.cancel()
        hintFetchTask = nil
        baselinePollTask?.cancel()
        baselinePollTask = nil

        if let gw = gateway, authorityHeld {
            try? await gw.authority.release(AuthorityRelease(scope: .nowPlayingPlayback))
            try? await gw.authority.release(AuthorityRelease(scope: .nowPlayingMetadata))
        }
        authorityHeld = false

        nowPlayingObserver?(nil)
        nowPlayingObserver = nil

        client = nil
        gateway = nil
    }

    public func setNowPlayingObserver(_ observer: @escaping @Sendable (GlueNowPlaying?) -> Void) async {
        nowPlayingObserver = observer
    }

    // MARK: - inbound dispatch

    public func play(_ uri: PlayUri) async throws {
        guard let client else { throw GlueError.detached }
        if let context = uri.context, let parsed = SpotifyURI(context.contextUri) {
            let skip = SpotifyURI(uri.uri)
            await client.player.play(uri: parsed, skipToUri: skip)
        } else if let parsed = SpotifyURI(uri.uri) {
            await client.player.play(uri: parsed)
        } else {
            throw GlueError.notImplemented
        }
    }

    public func pause() async throws {
        guard let client else { throw GlueError.detached }
        await client.player.pause()
    }

    public func resume() async throws {
        guard let client else { throw GlueError.detached }
        await client.player.resume()
    }

    public func skipNext() async throws {
        guard let client else { throw GlueError.detached }
        await client.player.skipNext()
    }

    public func skipPrev() async throws {
        guard let client else { throw GlueError.detached }
        await client.player.skipPrevious()
    }

    public func seekTo(_ ms: UInt32) async throws {
        guard let client else { throw GlueError.detached }
        await client.player.seek(positionMs: Int(ms))
    }

    public func setShuffle(_ on: Bool) async throws {
        guard let client else { throw GlueError.detached }
        await client.player.setShuffle(on)
    }

    public func setRepeat(_ mode: WireRepeat) async throws {
        guard let client else { throw GlueError.detached }
        let mapped: SpotinyRepeat = switch mode {
        case .off: .off
        case .all: .context
        case .one: .track
        }
        await client.player.setRepeatMode(mapped)
    }

    public func handlePlaybackHint(_ hint: PlaybackHint) async {
        // Filter for Spotify-app activity only. Other-app hints are not
        // ours to react to. Hints with an unset bundle (rare) also drop.
        guard hint.appBundle == spotifyAppBundle else { return }

        hintFetchTask?.cancel()
        hintFetchTask = Task { [weak self] in
            try? await Task.sleep(nanoseconds: hintDebounceNanos)
            if Task.isCancelled { return }
            await self?.fetchAndDispatch()
        }
    }

    public func asset(id: String) async throws -> AssetBytes? {
        guard id.hasPrefix(assetIdPrefix) else { return nil }
        let encoded = String(id.dropFirst(assetIdPrefix.count))
        guard let urlString = encoded.removingPercentEncoding,
              let url = URL(string: urlString) else { return nil }
        let (data, response) = try await urlSession.data(from: url)
        let mime = (response as? HTTPURLResponse)?.value(forHTTPHeaderField: "Content-Type")
        return AssetBytes(bytes: data, mime: mime)
    }

    /// Pull the canonical playback state from `/v1/me/player` and route
    /// it through the same path dealer-WS pushes take. Both hint-driven
    /// and baseline-poll fetches funnel here.
    fileprivate func fetchAndDispatch() async {
        guard let client else { return }
        guard let state = await client.player.getPlaybackState() else { return }
        handleStateUpdate(state)
    }

    // MARK: - outbound

    fileprivate func handleStateUpdate(_ state: Spotiny.PlayerState) {
        guard let gateway else { return }
        let update = Self.makeUpdate(from: state)
        let artworkUrl = state.item.flatMap(Self.rawArtworkURL(for:))
        nowPlayingObserver?(GlueNowPlaying(update: update, artworkUrl: artworkUrl))

        let nowPlaying = state.is_playing
        Task { [weak self] in
            try? await gateway.player.delta(update)
            guard let self else { return }
            if nowPlaying {
                try? await gateway.authority.claim(AuthorityClaim(scope: .nowPlayingPlayback))
                try? await gateway.authority.claim(AuthorityClaim(scope: .nowPlayingMetadata))
                authorityHeld = true
                startBaselinePollIfNeeded()
            } else if authorityHeld {
                try? await gateway.authority.release(AuthorityRelease(scope: .nowPlayingPlayback))
                try? await gateway.authority.release(AuthorityRelease(scope: .nowPlayingMetadata))
                authorityHeld = false
                stopBaselinePoll()
            }
        }
    }

    fileprivate func handleSocketDown() {
        nowPlayingObserver?(nil)
        stopBaselinePoll()
        guard let gateway, authorityHeld else { return }
        authorityHeld = false
        Task {
            try? await gateway.authority.release(AuthorityRelease(scope: .nowPlayingPlayback))
            try? await gateway.authority.release(AuthorityRelease(scope: .nowPlayingMetadata))
        }
    }

    private func startBaselinePollIfNeeded() {
        guard baselinePollTask == nil else { return }
        baselinePollTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: pollIntervalNanos)
                if Task.isCancelled { return }
                await self?.fetchAndDispatch()
            }
        }
    }

    private func stopBaselinePoll() {
        baselinePollTask?.cancel()
        baselinePollTask = nil
    }

    private static func makeUpdate(from state: Spotiny.PlayerState) -> NowPlayingUpdate {
        let media: MediaItemUpdate? = state.item.map { item in
            let title = item.name
            let artist = item.artists.map(\.name).joined(separator: ", ")
            let album: String? = if case let .track(track) = item { track.album?.name } else { nil }
            return MediaItemUpdate(
                persistentId: item.uri,
                title: title.isEmpty ? nil : title,
                album: album,
                albumArtist: nil,
                artist: artist.isEmpty ? nil : artist,
                liked: nil,
                artworkId: artworkId(for: item),
                durationMs: UInt32(max(item.duration_ms, 0)),
                mediaTypes: nil,
                trackNumber: nil,
                trackCount: nil,
                isLikeSupported: nil,
                isBanSupported: nil,
                isBanned: nil,
                isResidentOnDevice: nil,
                chapterCount: nil
            )
        }

        let allowSeek = state.actions?.disallows?.seeking.map { !$0 } ?? true
        let playback = PlaybackUpdate(
            playing: state.is_playing,
            positionMs: UInt32(max(state.progress_ms, 0)),
            shuffle: state.shuffle_state,
            shuffleMode: state.shuffle_state ? .songs : .off,
            repeat: mapRepeat(state.repeat_state),
            appBundle: "com.spotify.client",
            appDisplayName: "Spotify",
            queueIndex: nil,
            queueCount: nil,
            queueChapterIndex: nil,
            playbackSpeed: nil,
            setElapsedTimeAvailable: allowSeek,
            queueListAvail: nil,
            appleMusicRadioAd: nil,
            appleMusicRadioStationName: nil
        )

        return NowPlayingUpdate(mediaItem: media, playback: playback)
    }

    private static func artworkId(for item: PlayerItem) -> String? {
        guard let pick = rawArtworkURL(for: item),
              let encoded = pick.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed)
        else { return nil }
        return assetIdPrefix + encoded
    }

    fileprivate static func rawArtworkURL(for item: PlayerItem) -> String? {
        let urls = item.imageUrl
        if !urls.large.isEmpty { return urls.large }
        if !urls.medium.isEmpty { return urls.medium }
        if !urls.small.isEmpty { return urls.small }
        return nil
    }

    private static func mapRepeat(_ mode: SpotinyRepeat) -> WireRepeat {
        switch mode {
        case .off: .off
        case .track: .one
        case .context: .all
        }
    }
}

extension SpotifyGlue: SpotinyDelegate {
    public func authDidRefresh(accessToken: String, refreshToken: String) {
        onTokensRefreshed?(accessToken, refreshToken)
    }

    public func socketDidConnect() {}

    public func socketDidDisconnect() {
        handleSocketDown()
    }

    public func playerStateUpdated(oldState _: Spotiny.PlayerState?, newState: Spotiny.PlayerState) {
        handleStateUpdate(newState)
    }
}
