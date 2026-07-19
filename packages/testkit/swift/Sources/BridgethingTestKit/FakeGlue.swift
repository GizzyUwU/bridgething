import BridgethingGateway
import BridgethingGlue
import BridgethingLyrics
import BridgethingSchema
import Foundation
import os

/// Verb calls a `FakeGlue` observed, for test assertions.
public enum FakeGlueCall: Sendable, Equatable {
    case attach
    case detach
    case peerConnected(allowAutoResume: Bool)
    case play(String)
    case queue(String)
    case pause
    case resume
    case skipNext
    case skipPrev
    case skipToIndex(UInt32)
    case seekTo(UInt32)
    case setShuffle(Bool)
    case setRepeat(RepeatMode)
    case setSpeed(Float)
    case setCrossfade(UInt32?)
    case browse
    case search(String)
    case recommendations
    case favoritesList
    case favoritesContains([String])
    case favoritesToggle(String)
    case favoritesSet(String, Bool)
    case favoritesSetMany(Int)
    case asset(String)
}

public final class FakeGlue: BridgethingGlue, @unchecked Sendable {
    public static let name: String = "fake"
    public static let displayName: String = "Fake"

    public struct Behaviors: Sendable {
        public var browse: (@Sendable (LibraryBrowseRequest) async throws -> BrowseResult)?
        public var search: (@Sendable (LibrarySearchRequest) async throws -> SearchResult)?
        public var recommendations: (@Sendable (LibraryRecommendationsRequest) async throws -> RecommendationsResult)?
        public var favoritesList: (@Sendable (LibraryFavoritesListRequest) async throws -> FavoritesPage)?
        public var favoritesContains: (@Sendable (LibraryFavoritesContainsRequest) async throws -> [Bool])?
        public var asset: (@Sendable (String) async throws -> AssetBytes?)?
        public init() {}
    }

    public let capabilities: GlueCapabilities
    public let uriSchemes: [String]
    public let musicProvider: MusicProvider
    public let lyricsSupported: Bool

    private let behaviors: Behaviors
    private let _calls = OSAllocatedUnfairLock(initialState: [FakeGlueCall]())

    public var calls: [FakeGlueCall] {
        _calls.withLock { $0 }
    }

    private func record(_ call: FakeGlueCall) {
        _calls.withLock { $0.append(call) }
    }

    public init(
        behaviors: Behaviors = Behaviors(),
        capabilities: GlueCapabilities = [.streaming, .queue, .albumArt, .library, .playlists, .recommendations],
        uriSchemes: [String] = ["fake"],
        musicProvider: MusicProvider = .none,
        lyricsSupported: Bool = false
    ) {
        self.behaviors = behaviors
        self.capabilities = capabilities
        self.uriSchemes = uriSchemes
        self.musicProvider = musicProvider
        self.lyricsSupported = lyricsSupported
    }

    public func attach(gateway _: BridgethingGateway) async throws { record(.attach) }
    public func detach() async { record(.detach) }
    public func handlePeerConnected(allowAutoResume: Bool) async { record(.peerConnected(allowAutoResume: allowAutoResume)) }

    // MARK: - player

    public func play(_ uri: PlayUri) async throws { record(.play(uri.uri)) }
    public func queue(_ req: QueueUri) async throws { record(.queue(req.uri)) }
    public func pause() async throws { record(.pause) }
    public func resume() async throws { record(.resume) }
    public func skipNext() async throws { record(.skipNext) }
    public func skipPrev() async throws { record(.skipPrev) }
    public func skipToIndex(_ index: UInt32) async throws { record(.skipToIndex(index)) }
    public func seekTo(_ ms: UInt32) async throws { record(.seekTo(ms)) }
    public func setShuffle(_ on: Bool) async throws { record(.setShuffle(on)) }
    public func setRepeat(_ mode: RepeatMode) async throws { record(.setRepeat(mode)) }
    public func setSpeed(_ speed: Float) async throws { record(.setSpeed(speed)) }
    public func setCrossfade(_ durationMs: UInt32?) async throws { record(.setCrossfade(durationMs)) }

    // MARK: - library

    public func browse(_ req: LibraryBrowseRequest) async throws -> BrowseResult {
        record(.browse)
        guard let behaviors = behaviors.browse else { throw GlueError.notImplemented }
        return try await behaviors(req)
    }

    public func search(_ req: LibrarySearchRequest) async throws -> SearchResult {
        record(.search(req.query))
        guard let behaviors = behaviors.search else { throw GlueError.notImplemented }
        return try await behaviors(req)
    }

    public func recommendations(_ req: LibraryRecommendationsRequest) async throws -> RecommendationsResult {
        record(.recommendations)
        guard let behaviors = behaviors.recommendations else { throw GlueError.notImplemented }
        return try await behaviors(req)
    }

    public func favoritesList(_ req: LibraryFavoritesListRequest) async throws -> FavoritesPage {
        record(.favoritesList)
        guard let behaviors = behaviors.favoritesList else { throw GlueError.notImplemented }
        return try await behaviors(req)
    }

    public func favoritesContains(_ req: LibraryFavoritesContainsRequest) async throws -> [Bool] {
        record(.favoritesContains(req.uris))
        guard let behaviors = behaviors.favoritesContains else { throw GlueError.notImplemented }
        return try await behaviors(req)
    }

    public func favoritesToggle(_ item: ItemRef) async throws { record(.favoritesToggle(item.uri)) }
    public func favoritesSet(_ item: ItemRef, liked: Bool) async throws { record(.favoritesSet(item.uri, liked)) }
    public func favoritesSetMany(_ entries: [FavoritesSet]) async throws { record(.favoritesSetMany(entries.count)) }

    // MARK: - asset / lyrics

    public func asset(id: String) async throws -> AssetBytes? {
        record(.asset(id))
        guard let behaviors = behaviors.asset else { return nil }
        return try await behaviors(id)
    }

    public func lyrics(for _: BridgethingLyrics.TrackIdentity) async throws -> BridgethingLyrics.Lyrics? { nil }
}
