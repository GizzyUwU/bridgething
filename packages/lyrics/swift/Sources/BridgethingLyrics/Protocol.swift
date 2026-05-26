import Foundation

/// Pluggable lyrics fetcher. Different impls hit different sources;
/// the consumer decides the fallback chain.
public protocol LyricsResolver: Sendable {
    var name: String { get }
    func lyrics(for track: TrackIdentity) async -> Lyrics?
}

/// Identifies a track for lyrics lookup. All fields are populated when available;
/// resolvers use what they need and ignore the rest.
public struct TrackIdentity: Sendable, Hashable {
    public let artist: String
    public let track: String
    public let album: String?
    public let durationMs: Int?
    public let isrc: String?

    public init(
        artist: String,
        track: String,
        album: String? = nil,
        durationMs: Int? = nil,
        isrc: String? = nil
    ) {
        self.artist = artist
        self.track = track
        self.album = album
        self.durationMs = durationMs
        self.isrc = isrc
    }
}

public struct Lyrics: Sendable, Hashable {
    public let synced: [LyricLine]?
    public let plain: String?
    public let source: String

    public init(synced: [LyricLine]?, plain: String?, source: String) {
        self.synced = synced
        self.plain = plain
        self.source = source
    }
}

public struct LyricLine: Sendable, Hashable {
    public let startMs: Int
    public let text: String

    public init(startMs: Int, text: String) {
        self.startMs = startMs
        self.text = text
    }
}
