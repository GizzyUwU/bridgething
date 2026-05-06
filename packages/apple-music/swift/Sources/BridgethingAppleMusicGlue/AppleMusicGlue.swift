import BridgethingGateway
import BridgethingGlue
import BridgethingSchema
import Foundation

public final class AppleMusicGlue: BridgethingGlue, @unchecked Sendable {
    public static let name: String = "apple-music"
    public static let displayName: String = "Apple Music"

    public let capabilities: GlueCapabilities = []
    public let uriSchemes: [String] = []
    public let musicProvider: MusicProvider = .appleMusic
    public let lyricsSupported: Bool = false

    public init() {}

    public func attach(gateway _: BridgethingGateway) async throws {
        throw GlueError.notImplemented
    }

    public func detach() async {}
}
