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

    /// Inbound transport-control verbs. Default impls throw
    /// `GlueError.notImplemented`; concrete glues override the verbs they
    /// support. The companion's central dispatcher routes inbound
    /// `BridgeToGatewayPlayerMsg` variants to these.
    func play(_ uri: PlayUri) async throws
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
    func asset(id _: String) async throws -> AssetBytes? { nil }
    func lyrics(for _: BridgethingLyrics.TrackIdentity) async throws -> BridgethingLyrics.Lyrics? { nil }
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
