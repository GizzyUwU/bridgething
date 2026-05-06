import BridgethingGateway
import BridgethingGlue
import BridgethingSchema
import Foundation

public final class TidalGlue: BridgethingGlue, @unchecked Sendable {
    public static let name: String = "tidal"
    public static let displayName: String = "Tidal"

    public let capabilities: GlueCapabilities = []
    public let uriSchemes: [String] = []
    public let musicProvider: MusicProvider = .tidal
    public let lyricsSupported: Bool = false

    public init() {}

    public func attach(gateway _: BridgethingGateway) async throws {
        throw GlueError.notImplemented
    }

    public func detach() async {}
}
