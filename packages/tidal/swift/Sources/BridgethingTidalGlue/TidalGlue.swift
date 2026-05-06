import BridgethingGateway
import BridgethingGlue
import BridgethingSchema
import Foundation

/// Tidal glue stub. Real impl will use Tidal's OAuth + Web API. Surfaced
/// from day one so the companion app's settings UI can list it as
/// "coming soon" and the architecture's intent is visible to anyone
/// cloning the repo.
public final class TidalGlue: BridgethingGlue, @unchecked Sendable {
    public static let name: String = "tidal"
    public static let displayName: String = "Tidal"

    public let capabilities: GlueCapabilities = []

    public init() {}

    public func attach(gateway: BridgethingGateway) async throws {
        throw GlueError.notImplemented
    }

    public func detach() async {}
}
