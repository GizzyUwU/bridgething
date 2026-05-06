import BridgethingGateway
import BridgethingGlue
import BridgethingSchema
import Foundation
import Spotiny

/// `BridgethingGlue` impl backed by the spotiny Web API + dealer WS client.
/// `attach` body lands in a follow-up slice; the type, capabilities, and
/// init signature are stable from this round so downstream wiring can
/// reference them.
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

    private let authenticator: any OAuthAuthenticator
    private var client: SpotinyClient?

    public init(authenticator: any OAuthAuthenticator) {
        self.authenticator = authenticator
    }

    public func attach(gateway: BridgethingGateway) async throws {
        throw GlueError.notImplemented
    }

    public func detach() async {
        client = nil
    }
}
