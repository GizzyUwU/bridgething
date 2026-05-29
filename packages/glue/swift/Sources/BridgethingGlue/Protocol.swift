import BridgethingGateway
import BridgethingLyrics
import BridgethingSchema
import Foundation

/// Pluggable music-provider abstraction over a connected `BridgethingGateway`.
///
/// The companion calls `attach(gateway:)` after the gateway is running and
/// dispatches inbound player verbs, asset requests, and lyrics requests to
/// the corresponding methods. Outbound (NowPlaying delta, authority claim/release)
/// is the glue's own responsibility once it holds the gateway reference.
///
/// Glues contribute `uriSchemes`, `musicProvider`, and `lyricsSupported`;
/// capability composition is companion-level.
public protocol BridgethingGlue: Sendable {
    static var name: String { get }
    static var displayName: String { get }

    var capabilities: GlueCapabilities { get }
    var uriSchemes: [String] { get }
    var musicProvider: MusicProvider { get }
    var lyricsSupported: Bool { get }

    func attach(gateway: BridgethingGateway) async throws
    func detach() async

    /// Subscribe to NowPlaying updates. `nil` means nothing is playing or the source went away.
    /// Default impl is a no-op.
    func setNowPlayingObserver(_ observer: @escaping @Sendable (GlueNowPlaying?) -> Void) async

    /// Inbound transport-control verbs. Default impls throw `GlueError.notImplemented`;
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

    /// Library verbs; default impls throw `GlueError.notImplemented`.
    func browse(_ req: LibraryBrowseRequest) async throws -> BrowseResult
    func search(_ req: LibrarySearchRequest) async throws -> SearchResult
    func recommendations(_ req: LibraryRecommendationsRequest) async throws -> RecommendationsResult
    func favoritesList(_ req: LibraryFavoritesListRequest) async throws -> FavoritesPage
    func favoritesContains(_ req: LibraryFavoritesContainsRequest) async throws -> [Bool]
    func favoritesToggle(_ item: ItemRef) async throws
    func favoritesSet(_ item: ItemRef, liked: Bool) async throws
    func favoritesSetMany(_ entries: [FavoritesSet]) async throws

    /// Subscribe to auth-lifecycle updates. The glue drives the lifecycle:
    /// `pending(nil)` while negotiating, `pending(prompt)` once a device-code
    /// prompt is available, `authenticated` after token exchange, `failed` on error.
    func setAuthObserver(_ observer: @escaping @Sendable (GlueAuthState) -> Void) async

    /// Subscribe to service-health updates: `ok` when the provider's API is
    /// responsive, `rateLimited`/`unreachable` when degraded. Distinct from auth.
    func setServiceHealthObserver(_ observer: @escaping @Sendable (GlueServiceHealth) -> Void) async

    /// Daemon-observed iAP2 playback hint. The hint is not authoritative; the glue
    /// should fetch from its own data source and push back via `gateway.player.delta`.
    /// Filter on `appBundle` to avoid spurious fetches from other apps. Default impl is a no-op.
    func handlePlaybackHint(_ hint: PlaybackHint) async

    /// Bytes for an asset id this glue produced. Return nil if the id isn't owned by this glue.
    func asset(id: String) async throws -> AssetBytes?

    /// Provider-native lyrics path. Return nil to fall through to the companion's `LyricsResolver`.
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
    func browse(_: LibraryBrowseRequest) async throws -> BrowseResult { throw GlueError.notImplemented }
    func search(_: LibrarySearchRequest) async throws -> SearchResult { throw GlueError.notImplemented }
    func recommendations(_: LibraryRecommendationsRequest) async throws -> RecommendationsResult { throw GlueError.notImplemented }
    func favoritesList(_: LibraryFavoritesListRequest) async throws -> FavoritesPage { throw GlueError.notImplemented }
    func favoritesContains(_: LibraryFavoritesContainsRequest) async throws -> [Bool] { throw GlueError.notImplemented }
    func favoritesToggle(_: ItemRef) async throws { throw GlueError.notImplemented }
    func favoritesSet(_: ItemRef, liked _: Bool) async throws { throw GlueError.notImplemented }
    func favoritesSetMany(_: [FavoritesSet]) async throws { throw GlueError.notImplemented }
    func handlePlaybackHint(_: PlaybackHint) async {}
    func asset(id _: String) async throws -> AssetBytes? { nil }
    func lyrics(for _: BridgethingLyrics.TrackIdentity) async throws -> BridgethingLyrics.Lyrics? { nil }
    func setNowPlayingObserver(_: @escaping @Sendable (GlueNowPlaying?) -> Void) async {}

    /// Default for glues without an auth surface: report ready immediately.
    func setAuthObserver(_ observer: @escaping @Sendable (GlueAuthState) -> Void) async {
        observer(.authenticated)
    }

    /// Default for glues without a health surface: always healthy.
    func setServiceHealthObserver(_ observer: @escaping @Sendable (GlueServiceHealth) -> Void) async {
        observer(.ok)
    }
}

/// Auth lifecycle state surfaced to the host. Intentionally narrower than the wire types
/// so glues don't depend on the schema package.
public enum GlueAuthState: Sendable {
    case pending(GlueDeviceCodePrompt?)
    case authenticated
    case failed(String)
}

/// Provider service health, surfaced alongside (not inside) auth state.
public enum GlueServiceHealth: Sendable {
    case ok
    case rateLimited(retryAfterSeconds: Int)
    case unreachable
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

/// NowPlaying snapshot surfaced by the active glue. Carries the raw artwork URL so the
/// UI can load directly from the provider's CDN.
public struct GlueNowPlaying: Sendable {
    public let update: NowPlayingUpdate
    public let artworkUrl: String?

    public init(update: NowPlayingUpdate, artworkUrl: String? = nil) {
        self.update = update
        self.artworkUrl = artworkUrl
    }
}

/// Bytes payload returned from `BridgethingGlue.asset(id:)`.
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
