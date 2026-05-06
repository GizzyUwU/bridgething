import BridgethingGateway
import Foundation

/// Pluggable music-provider abstraction over a connected `BridgethingGateway`.
/// One concrete impl per music service (Spotify, Apple Music, Tidal, ...).
/// At most one glue is attached to a gateway at a time; switching providers
/// goes detach -> attach.
public protocol BridgethingGlue: Sendable {
    static var name: String { get }
    static var displayName: String { get }

    var capabilities: GlueCapabilities { get }

    func attach(gateway: BridgethingGateway) async throws
    func detach() async
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
