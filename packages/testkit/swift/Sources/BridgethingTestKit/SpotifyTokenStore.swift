import Foundation

public struct SpotifyTokens: Codable, Sendable {
    public let accessToken: String
    public let refreshToken: String

    public init(accessToken: String, refreshToken: String) {
        self.accessToken = accessToken
        self.refreshToken = refreshToken
    }
}

public enum SpotifyTokenStore {
    public static func path() -> URL {
        let env = ProcessInfo.processInfo.environment
        if let explicit = env["BRIDGETHING_TEST_SPOTIFY_TOKENS"], !explicit.isEmpty {
            return URL(fileURLWithPath: (explicit as NSString).expandingTildeInPath)
        }
        let base: URL
        if let xdg = env["XDG_CACHE_HOME"], !xdg.isEmpty {
            base = URL(fileURLWithPath: xdg)
        } else {
            base = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".cache")
        }
        return base
            .appendingPathComponent("bridgething-test")
            .appendingPathComponent("spotify-tokens.json")
    }

    public static func load() -> SpotifyTokens? {
        guard let data = try? Data(contentsOf: path()) else { return nil }
        return try? JSONDecoder().decode(SpotifyTokens.self, from: data)
    }

    public static func save(_ tokens: SpotifyTokens) throws {
        let url = path()
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        let data = try JSONEncoder().encode(tokens)
        try data.write(to: url, options: .atomic)
    }
}
