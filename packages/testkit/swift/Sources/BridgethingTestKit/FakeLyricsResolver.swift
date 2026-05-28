import BridgethingLyrics
import Foundation

public final class FakeLyricsResolver: LyricsResolver, @unchecked Sendable {
    public let name: String = "fake"
    private let canned: Lyrics?

    public init(canned: Lyrics? = nil) {
        self.canned = canned
    }

    public func lyrics(for _: TrackIdentity) async -> Lyrics? {
        canned
    }
}
