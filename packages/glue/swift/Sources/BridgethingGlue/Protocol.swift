import BridgethingGateway
import BridgethingLyrics
import BridgethingSchema
import Foundation

/// Pluggable music-provider abstraction over a connected `BridgethingGateway`.
///
/// Glues are lifecycle-managed by `BridgethingCompanion`: the companion calls
/// `attach(gateway:)` after the gateway is running, and dispatches inbound
/// player verbs / asset / lyrics requests to the corresponding methods.
/// Outbound (NowPlaying delta, authority claim/release) is the glue's own
/// responsibility once it holds the gateway reference.
///
/// Capabilities are composed by the companion. Glues contribute
/// `uriSchemes`, `musicProvider`, and `lyricsSupported`; everything else
/// (geo, net, audio_tts, ...) is companion-level and ignored here.
public protocol BridgethingGlue: Sendable {
    static var name: String { get }
    static var displayName: String { get }

    var capabilities: GlueCapabilities { get }
    var uriSchemes: [String] { get }
    var musicProvider: MusicProvider { get }
    var lyricsSupported: Bool { get }

    func attach(gateway: BridgethingGateway) async throws
    func detach() async

    /// Subscribe to NowPlaying mirror updates. The active glue invokes the
    /// observer with deltas alongside its outbound `gateway.player.delta`
    /// events; the companion forwards these to the phone-side UI shell.
    /// `nil` means "nothing playing / source went away". Default impl is
    /// no-op for stub glues.
    func setNowPlayingObserver(_ observer: @escaping @Sendable (GlueNowPlaying?) -> Void) async

    /// Inbound transport-control verbs. Default impls throw
    /// `GlueError.notImplemented`; concrete glues override the verbs they
    /// support. The companion's central dispatcher routes inbound
    /// `BridgeToGatewayPlayerMsg` variants to these.
    func play(_ uri: PlayUri) async throws
    func queue(_ req: QueueUri) async throws
    func pause() async throws
    func resume() async throws
    func skipNext() async throws
    func skipPrev() async throws
    func skipToIndex(_ index: UInt32) async throws
    func seekTo(_ ms: UInt32) async throws
    func setShuffle(_ on: Bool) async throws
    func setRepeat(_ mode: BridgethingSchema.RepeatMode) async throws
    func setSpeed(_ speed: Float) async throws
    func setCrossfade(_ durationMs: UInt32?) async throws

    /// Subscribe to auth-lifecycle updates. The active glue is responsible
    /// for driving this — `pending(prompt: nil)` while it negotiates,
    /// `pending(prompt: ...)` once it has a device-code prompt to show
    /// the user, `authenticated` after token exchange succeeds (refresh
    /// or fresh authorize), and `failed(reason)` on error. Stub glues
    /// without an auth surface emit `authenticated` from the default
    /// extension below so the host can advance immediately.
    func setAuthObserver(_ observer: @escaping @Sendable (GlueAuthState) -> Void) async

    /// Daemon-observed iAP2 playback hint. Fires when the daemon notices
    /// the iPhone's NowPlaying state changed in a way the companion can't
    /// see directly (track change, play-state flip, app switch). The hint
    /// itself is not authoritative state - the glue is expected to react
    /// by fetching from its own data source (e.g. Spotify Web API) and
    /// pushing the result back via `gateway.player.delta`. Filter on
    /// `appBundle` so other-app hints don't trigger spurious fetches.
    /// Default impl is no-op.
    func handlePlaybackHint(_ hint: PlaybackHint) async

    /// Bytes for an asset id this glue produced (e.g.
    /// `"spotify/img/<base64url>"`). Return nil if the id isn't this
    /// glue's; the companion replies `AssetNotFound` in that case.
    func asset(id: String) async throws -> AssetBytes?

    /// Provider-native lyrics path. Return nil to fall through to the
    /// companion's injected `LyricsResolver` (lrclib by default).
    func lyrics(for track: BridgethingLyrics.TrackIdentity) async throws -> BridgethingLyrics.Lyrics?
}

public extension BridgethingGlue {
    func play(_: PlayUri) async throws { throw GlueError.notImplemented }
    func queue(_: QueueUri) async throws { throw GlueError.notImplemented }
    func pause() async throws { throw GlueError.notImplemented }
    func resume() async throws { throw GlueError.notImplemented }
    func skipNext() async throws { throw GlueError.notImplemented }
    func skipPrev() async throws { throw GlueError.notImplemented }
    func skipToIndex(_: UInt32) async throws { throw GlueError.notImplemented }
    func seekTo(_: UInt32) async throws { throw GlueError.notImplemented }
    func setShuffle(_: Bool) async throws { throw GlueError.notImplemented }
    func setRepeat(_: BridgethingSchema.RepeatMode) async throws { throw GlueError.notImplemented }
    func setSpeed(_: Float) async throws { throw GlueError.notImplemented }
    func setCrossfade(_: UInt32?) async throws { throw GlueError.notImplemented }
    func handlePlaybackHint(_: PlaybackHint) async {}
    func asset(id _: String) async throws -> AssetBytes? { nil }
    func lyrics(for _: BridgethingLyrics.TrackIdentity) async throws -> BridgethingLyrics.Lyrics? { nil }
    func setNowPlayingObserver(_: @escaping @Sendable (GlueNowPlaying?) -> Void) async {}

    /// Default for glues without an auth surface: report ready
    /// immediately. Glues with real OAuth (Spotify, Apple Music when it
    /// lands) override this to drive the lifecycle.
    func setAuthObserver(_ observer: @escaping @Sendable (GlueAuthState) -> Void) async {
        observer(.authenticated)
    }
}

/// Auth lifecycle the active glue surfaces to the host. Mirrors the
/// shape the React Native session bridge publishes to the companion app
/// UI; intentionally narrower than the wire types so glues don't have
/// to depend on the schema package.
public enum GlueAuthState: Sendable {
    case pending(GlueDeviceCodePrompt?)
    case authenticated
    case failed(String)
}

public struct GlueDeviceCodePrompt: Sendable {
    public let userCode: String
    public let verificationURL: URL
    public let verificationURLComplete: URL

    public init(
        userCode: String,
        verificationURL: URL,
        verificationURLComplete: URL
    ) {
        self.userCode = userCode
        self.verificationURL = verificationURL
        self.verificationURLComplete = verificationURLComplete
    }
}

/// NowPlaying snapshot the active glue surfaces to the companion. Wraps
/// the wire `NowPlayingUpdate` with the raw artwork URL so phone-side UI
/// can load directly from the provider's CDN, bypassing the on-device
/// asset-cache indirection.
public struct GlueNowPlaying: Sendable {
    public let update: NowPlayingUpdate
    public let artworkUrl: String?

    public init(update: NowPlayingUpdate, artworkUrl: String? = nil) {
        self.update = update
        self.artworkUrl = artworkUrl
    }
}

/// Bytes payload returned from `BridgethingGlue.asset(id:)`. The companion
/// converts this into the wire `AssetGotReply`.
public struct AssetBytes: Sendable {
    public let bytes: Data
    public let mime: String?
    public init(bytes: Data, mime: String? = nil) {
        self.bytes = bytes
        self.mime = mime
    }
}

public struct GlueCapabilities: OptionSet, Sendable {
    public let rawValue: UInt32
    public init(rawValue: UInt32) { self.rawValue = rawValue }

    public static let streaming = GlueCapabilities(rawValue: 1 << 0)
    public static let queue = GlueCapabilities(rawValue: 1 << 1)
    public static let lyrics = GlueCapabilities(rawValue: 1 << 2)
    public static let albumArt = GlueCapabilities(rawValue: 1 << 3)
    public static let recommendations = GlueCapabilities(rawValue: 1 << 4)
    public static let recentlyPlayed = GlueCapabilities(rawValue: 1 << 5)
    public static let library = GlueCapabilities(rawValue: 1 << 6)
    public static let playlists = GlueCapabilities(rawValue: 1 << 7)
}

public enum GlueError: Error, Sendable {
    case notImplemented
    case notAuthenticated
    case detached
    case underlying(any Error & Sendable)
}
