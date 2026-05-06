import BridgethingGateway
import BridgethingGlue
import BridgethingSchema
import Foundation

/// Apple Music glue stub. Real impl will use MusicKit + the user's signed-in
/// Apple Music subscription. Surfaced from day one so the companion app's
/// settings UI can list it as "coming soon" and the architecture's intent
/// is visible to anyone cloning the repo.
public final class AppleMusicGlue: BridgethingGlue, @unchecked Sendable {
    public static let name: String = "apple-music"
    public static let displayName: String = "Apple Music"

    public let capabilities: GlueCapabilities = []

    public init() {}

    public func attach(gateway: BridgethingGateway) async throws {
        throw GlueError.notImplemented
    }

    public func detach() async {}
}
