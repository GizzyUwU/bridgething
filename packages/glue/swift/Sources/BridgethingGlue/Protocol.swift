import BridgethingGateway
import BridgethingLyrics
import BridgethingSchema
import Foundation

public protocol BridgethingGlue: NowPlayingTransport {
    static var name: String { get }
    static var displayName: String { get }

    var capabilities: GlueCapabilities { get }
    var uriSchemes: [String] { get }
    var musicProvider: MusicProvider { get }
    var lyricsSupported: Bool { get }

    func attach(gateway: BridgethingGateway) async throws
    func detach() async

    func setNowPlayingObserver(_ observer: @escaping @Sendable (GlueNowPlaying?) -> Void) async

    func setNowPlayingSink(_ sink: (any NowPlayingSink)?) async

    func setArtProfile(heroPx: Int, thumbPx: Int) async

    func handlePeerConnected(allowAutoResume: Bool) async

    func ownsVolume() async -> Bool
    func volumeUp() async throws
    func volumeDown() async throws
    func setVolume(_ level: Float) async throws

    var supportsPlaybackTargets: Bool { get }

    func transferTo(targetId: String) async throws

    func browse(_ req: LibraryBrowseRequest) async throws -> BrowseResult
    func resolveContext(_ uri: String) async throws -> ContextResolveReply
    func search(_ req: LibrarySearchRequest) async throws -> SearchResult
    func recommendations(_ req: LibraryRecommendationsRequest) async throws -> RecommendationsResult
    func favoritesList(_ req: LibraryFavoritesListRequest) async throws -> FavoritesPage
    func favoritesContains(_ req: LibraryFavoritesContainsRequest) async throws -> [Bool]
    func favoritesToggle(_ item: ItemRef) async throws
    func favoritesSet(_ item: ItemRef, liked: Bool) async throws
    func favoritesSetMany(_ entries: [FavoritesSet]) async throws

    func setAuthObserver(_ observer: @escaping @Sendable (GlueAuthState) -> Void) async
    func setServiceHealthObserver(_ observer: @escaping @Sendable (GlueServiceHealth) -> Void) async

    func asset(id: String) async throws -> AssetBytes?
    func lyrics(for track: BridgethingLyrics.TrackIdentity) async throws -> BridgethingLyrics.Lyrics?
    func debugState() async -> GlueDebugState
}

public struct GlueDebugState: Sendable {
    public let authorityPlaybackHeld: Bool
    public let authorityMetadataHeld: Bool

    public init(
        authorityPlaybackHeld: Bool = false,
        authorityMetadataHeld: Bool = false
    ) {
        self.authorityPlaybackHeld = authorityPlaybackHeld
        self.authorityMetadataHeld = authorityMetadataHeld
    }
}

public extension BridgethingGlue {
    func debugState() async -> GlueDebugState { GlueDebugState() }

    func ownsVolume() async -> Bool { false }
    func volumeUp() async throws { throw GlueError.notImplemented }
    func volumeDown() async throws { throw GlueError.notImplemented }
    func setVolume(_: Float) async throws { throw GlueError.notImplemented }
    var supportsPlaybackTargets: Bool { false }
    func transferTo(targetId _: String) async throws { throw GlueError.notImplemented }
    func browse(_: LibraryBrowseRequest) async throws -> BrowseResult { throw GlueError.notImplemented }
    func search(_: LibrarySearchRequest) async throws -> SearchResult { throw GlueError.notImplemented }
    func resolveContext(_: String) async throws -> ContextResolveReply { throw GlueError.notImplemented }
    func recommendations(_: LibraryRecommendationsRequest) async throws -> RecommendationsResult { throw GlueError.notImplemented }
    func favoritesList(_: LibraryFavoritesListRequest) async throws -> FavoritesPage { throw GlueError.notImplemented }
    func favoritesContains(_: LibraryFavoritesContainsRequest) async throws -> [Bool] { throw GlueError.notImplemented }
    func favoritesToggle(_: ItemRef) async throws { throw GlueError.notImplemented }
    func favoritesSet(_: ItemRef, liked _: Bool) async throws { throw GlueError.notImplemented }
    func favoritesSetMany(_: [FavoritesSet]) async throws { throw GlueError.notImplemented }
    func asset(id _: String) async throws -> AssetBytes? { nil }
    func lyrics(for _: BridgethingLyrics.TrackIdentity) async throws -> BridgethingLyrics.Lyrics? { nil }
    func setNowPlayingObserver(_: @escaping @Sendable (GlueNowPlaying?) -> Void) async {}
    func setNowPlayingSink(_: (any NowPlayingSink)?) async {}
    func setArtProfile(heroPx _: Int, thumbPx _: Int) async {}
    func handlePeerConnected(allowAutoResume _: Bool) async {}

    func setAuthObserver(_ observer: @escaping @Sendable (GlueAuthState) -> Void) async {
        observer(.authenticated)
    }

    func setServiceHealthObserver(_ observer: @escaping @Sendable (GlueServiceHealth) -> Void) async {
        observer(.ok)
    }
}

public enum GlueAuthState: Sendable {
    case pending(GlueDeviceCodePrompt?)
    case authenticated
    case failed(String)
}

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

public struct GlueNowPlaying: Sendable {
    public let update: NowPlayingUpdate
    public let artworkUrl: String?

    public init(update: NowPlayingUpdate, artworkUrl: String? = nil) {
        self.update = update
        self.artworkUrl = artworkUrl
    }
}

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
