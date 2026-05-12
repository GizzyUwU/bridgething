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

/// Closure the host supplies so the glue can build whichever
/// `OAuthAuthenticator` the host has configured (device-code or PKCE)
/// while still wiring the device-code prompt through the glue's own
/// auth-lifecycle observer. PKCE authenticators ignore the closure
/// argument (they present a WebView directly).
public typealias SpotifyAuthenticatorFactory = @Sendable (
    _ onPrompt: @escaping @Sendable (DeviceCodePrompt) async -> Void
) -> any OAuthAuthenticator

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
    public let lyricsSupported: Bool = false

    public typealias TokenCallback = @Sendable (_ accessToken: String, _ refreshToken: String) -> Void

    private let authenticatorFactory: SpotifyAuthenticatorFactory
    private let initialAccessToken: String
    private let initialRefreshToken: String
    private let onTokensRefreshed: TokenCallback?
    private let urlSession: URLSession

    private var client: SpotinyClient?
    private var gateway: BridgethingGateway?
    private var authorityHeld: Bool = false
    private var nowPlayingObserver: (@Sendable (GlueNowPlaying?) -> Void)?
    private var authObserver: (@Sendable (GlueAuthState) -> Void)?
    private var hintFetchTask: Task<Void, Never>?
    private var baselinePollTask: Task<Void, Never>?
    private var connectTask: Task<Void, Never>?

    public init(
        authenticatorFactory: @escaping SpotifyAuthenticatorFactory,
        accessToken: String = "",
        refreshToken: String = "",
        onTokensRefreshed: TokenCallback? = nil,
        urlSession: URLSession = .shared
    ) {
        self.authenticatorFactory = authenticatorFactory
        initialAccessToken = accessToken
        initialRefreshToken = refreshToken
        self.onTokensRefreshed = onTokensRefreshed
        self.urlSession = urlSession
    }

    public func attach(gateway: BridgethingGateway) async throws {
        if self.gateway != nil { await detach() }

        self.gateway = gateway

        // Tell the host we're starting; userCode prompt (if needed) will
        // arrive on the device-code path before tokens come back.
        authObserver?(.pending(nil))

        let authenticator = authenticatorFactory { [weak self] prompt in
            self?.handleDeviceCodePrompt(prompt)
        }

        let client = SpotinyClient(
            authenticator: authenticator,
            delegate: self,
            accessToken: initialAccessToken,
            refreshToken: initialRefreshToken
        )
        self.client = client

        // Run auth + dealer-socket connect in the background. We don't
        // await it here because the daytona/device-code client_id has no
        // dealer access — `socket.connect()` would block forever. Auth
        // lifecycle reaches the host through the spotiny delegate
        // (authDidRefresh / authDidFail), so blocking attach buys us
        // nothing.
        connectTask = Task { [weak client] in
            await client?.connect()
        }
    }

    public func detach() async {
        // Stop emitting auth state once we're tearing down; cancellation
        // races inside spotiny would otherwise fire authDidFail and emit
        // a ghost `failed` after the host has already moved to idle.
        authObserver = nil

        connectTask?.cancel()
        connectTask = nil
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

    public func setAuthObserver(_ observer: @escaping @Sendable (GlueAuthState) -> Void) async {
        authObserver = observer
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

    public func queue(_ req: QueueUri) async throws {
        guard let client else { throw GlueError.detached }
        if case .index = req.position { throw GlueError.notImplemented }
        guard let parsed = SpotifyURI(req.uri) else { throw GlueError.notImplemented }
        await client.player.addItemToQueue(uri: parsed)
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

    private func handleDeviceCodePrompt(_ prompt: DeviceCodePrompt) {
        authObserver?(.pending(GlueDeviceCodePrompt(
            userCode: prompt.userCode,
            verificationURL: prompt.verificationURL,
            verificationURLComplete: prompt.verificationURLPrefilled
        )))
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
        // Empty tokens here mean spotiny just cleared state because the
        // current attempt failed; the matching `authDidFail` will follow.
        // Don't emit `authenticated` until we actually have credentials.
        if !accessToken.isEmpty {
            authObserver?(.authenticated)
        }
    }

    public func authDidFail(reason: String) {
        handleSocketDown()
        authObserver?(.failed(reason))
    }

    public func socketDidConnect() {}

    public func socketDidDisconnect() {
        handleSocketDown()
    }

    public func playerStateUpdated(oldState _: Spotiny.PlayerState?, newState: Spotiny.PlayerState) {
        handleStateUpdate(newState)
    }
}
