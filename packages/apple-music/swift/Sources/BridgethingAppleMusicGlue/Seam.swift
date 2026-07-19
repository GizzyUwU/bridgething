import Foundation

public enum AmAuthStatus: Sendable {
    case notDetermined
    case authorized
    case denied
    case restricted
}

public enum AmRepeatMode: Sendable {
    case off
    case all
    case one
}

public struct AmEntry: Sendable, Equatable {
    public let uri: String?
    public let title: String
    public let artistName: String?
    public let albumName: String?
    public let artworkUrl: String?
    public let durationMs: UInt32?

    public init(
        uri: String?, title: String, artistName: String?, albumName: String?,
        artworkUrl: String?, durationMs: UInt32?
    ) {
        self.uri = uri
        self.title = title
        self.artistName = artistName
        self.albumName = albumName
        self.artworkUrl = artworkUrl
        self.durationMs = durationMs
    }
}

public struct AmPlayerSnapshot: Sendable {
    public let entry: AmEntry?
    public let playing: Bool
    public let positionMs: UInt32
    public let shuffle: Bool
    public let repeatMode: AmRepeatMode
    public let canSeek: Bool

    public init(
        entry: AmEntry?, playing: Bool, positionMs: UInt32, shuffle: Bool,
        repeatMode: AmRepeatMode, canSeek: Bool = true
    ) {
        self.entry = entry
        self.playing = playing
        self.positionMs = positionMs
        self.shuffle = shuffle
        self.repeatMode = repeatMode
        self.canSeek = canSeek
    }
}

public enum AmKind: String, Sendable {
    case song
    case album
    case playlist
    case artist
    case station
}

public struct AmItem: Sendable {
    public let uri: String
    public let kind: AmKind
    public let title: String
    public let subtitle: String?
    public let artistName: String?
    public let artistUri: String?
    public let albumName: String?
    public let albumUri: String?
    public let artworkUrl: String?
    public let durationMs: UInt32?
    public let trackCount: UInt32?

    public init(
        uri: String, kind: AmKind, title: String, subtitle: String? = nil,
        artistName: String? = nil, artistUri: String? = nil,
        albumName: String? = nil, albumUri: String? = nil,
        artworkUrl: String? = nil, durationMs: UInt32? = nil, trackCount: UInt32? = nil
    ) {
        self.uri = uri
        self.kind = kind
        self.title = title
        self.subtitle = subtitle
        self.artistName = artistName
        self.artistUri = artistUri
        self.albumName = albumName
        self.albumUri = albumUri
        self.artworkUrl = artworkUrl
        self.durationMs = durationMs
        self.trackCount = trackCount
    }
}

public struct AmPage: Sendable {
    public let items: [AmItem]
    public let total: UInt32?
    public let hasMore: Bool

    public init(items: [AmItem], total: UInt32?, hasMore: Bool) {
        self.items = items
        self.total = total
        self.hasMore = hasMore
    }
}

public struct AmShelf: Sendable {
    public let id: String
    public let title: String
    public let items: [AmItem]
    public let total: UInt32?

    public init(id: String, title: String, items: [AmItem], total: UInt32?) {
        self.id = id
        self.title = title
        self.items = items
        self.total = total
    }
}

public struct AmSearchResults: Sendable {
    public let songs: [AmItem]
    public let albums: [AmItem]
    public let artists: [AmItem]
    public let playlists: [AmItem]

    public init(songs: [AmItem], albums: [AmItem], artists: [AmItem], playlists: [AmItem]) {
        self.songs = songs
        self.albums = albums
        self.artists = artists
        self.playlists = playlists
    }
}

public enum AmPlayerError: Error, Sendable {
    case unavailable
    case itemNotFound(String)
    case underlying(String)
}

public protocol AppleMusicPlayerProviding: Sendable {
    func changes() -> AsyncStream<Void>
    func currentSnapshot() async -> AmPlayerSnapshot

    func play(contextUri: String, startAtUri: String?) async throws
    func queueInsert(uri: String, next: Bool) async throws

    func play() async throws
    func pause() async throws
    func skipNext() async throws
    func skipPrev() async throws
    func seek(toMs: UInt32) async throws
    func setShuffle(_ on: Bool) async throws
    func setRepeat(_ mode: AmRepeatMode) async throws

    func isOtherAudioPlaying() async -> Bool
}

public protocol AppleMusicAuthProviding: Sendable {
    func currentStatus() async -> AmAuthStatus
    func requestAuthorization() async -> AmAuthStatus
    func canPlayCatalogContent() async -> Bool?
}

public protocol AppleMusicLibraryProviding: Sendable {
    func libraryPlaylists(limit: UInt32, offset: UInt32) async throws -> AmPage
    func libraryAlbums(limit: UInt32, offset: UInt32) async throws -> AmPage
    func libraryArtists(limit: UInt32, offset: UInt32) async throws -> AmPage
    func recentlyPlayed(limit: UInt32, offset: UInt32) async throws -> AmPage
    func recommendations() async throws -> [AmShelf]
    func children(of uri: String, limit: UInt32, offset: UInt32) async throws -> AmPage
    func resolve(uri: String) async throws -> AmItem
    func search(query: String, limit: UInt32) async throws -> AmSearchResults

    func librarySongs(limit: UInt32, offset: UInt32) async throws -> AmPage
    func isFavorite(uris: [String]) async throws -> [Bool]
    func addFavorite(uri: String) async throws
}
