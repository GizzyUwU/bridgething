import Foundation
import Spotify

public protocol SpotifyClientProviding: AnyObject, Sendable {
    func connect() async throws
    func disconnect() async

    func pause() async throws
    func resume() async throws
    func skipNext() async throws
    func skipPrev() async throws
    func seek(positionMs: Int64) async throws
    func setShuffle(on: Bool) async throws
    func setRepeat(mode: Spotify.RepeatMode) async throws
    func queueUri(uri: String) async throws
    func play(uri: String, skipToUri: String?) async throws

    func product() async throws -> ProductState
    func rootBrowse() async throws -> [Shelf]
    func browse(nodeId: String, limit: UInt32, offset: UInt32) async throws -> BrowsePage
    func search(query: String, limit: UInt32) async throws -> SearchResults
    func resolveContext(uri: String) async throws -> Spotify.BrowseItem
    func favoritesContains(uris: [String]) async throws -> [Bool]
    func favoritesSet(uri: String, liked: Bool) async throws
    func favoritesList(limit: UInt32, offset: UInt32) async throws -> BrowsePage
}

extension SpotifyClient: SpotifyClientProviding {}

public typealias SpotifyClientFactory = @Sendable (
    _ store: any Spotify.TokenStore,
    _ observer: any Spotify.Observer
) -> any SpotifyClientProviding
