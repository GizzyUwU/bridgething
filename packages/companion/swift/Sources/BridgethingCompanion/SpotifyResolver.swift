import BridgethingSchema
import Foundation
#if canImport(FoundationNetworking)
    import FoundationNetworking
#endif

/// Resolve string slots on an `NluPrediction` into Spotify URIs via the
/// public Spotify Web API search endpoint. Decorates the prediction in
/// place; pure NLU intents (CLARIFY, WEBAPP_INTENT, fast-path-only)
/// pass through unchanged.
///
/// Auth: the host supplies an `accessTokenProvider` closure (typically
/// pointing at the daytona DeviceCodeAuthenticator's cached token). The
/// resolver doesn't refresh on its own - a 401 surfaces as ResolveError;
/// the host's auth layer is responsible for catching that and rotating.
///
/// Cache: in-process LRU of size `cacheCapacity` (default 100) keyed by
/// (entityType, normalizedQuery). Hits the same Spotify endpoint that
/// returns ranked search results, so cache hits are exact-query.
public actor SpotifyResolver {
    public enum ResolveError: Error, CustomStringConvertible {
        case authMissing
        case httpError(statusCode: Int, body: String)
        case invalidResponse(reason: String)
        case noMatch
        case ambiguous(top: [String])

        public var description: String {
            switch self {
            case .authMissing: return "spotify access token missing"
            case let .httpError(code, body): return "spotify http \(code): \(body.prefix(200))"
            case let .invalidResponse(reason): return "spotify response invalid: \(reason)"
            case .noMatch: return "spotify search returned no matches"
            case let .ambiguous(top): return "spotify search ambiguous; top=\(top)"
            }
        }
    }

    public struct Config: Sendable {
        public let baseURL: URL
        public let cacheCapacity: Int
        /// Curated playlist URIs for mood / genre slots when no Spotify
        /// search query maps cleanly. v0.1 ships a tiny seed table; richer
        /// resolution waits on real catalog work.
        public let moodPlaylists: [String: String]
        public let genrePlaylists: [String: String]

        public init(
            baseURL: URL = URL(string: "https://api.spotify.com/v1")!,
            cacheCapacity: Int = 100,
            moodPlaylists: [String: String] = [
                "chill": "spotify:playlist:37i9dQZF1DX4WYpdgoIcn6",
                "focus": "spotify:playlist:37i9dQZF1DWZeKCadgRdKQ",
                "workout": "spotify:playlist:37i9dQZF1DX76Wlfdnj7AP",
                "happy": "spotify:playlist:37i9dQZF1DXdPec7aLTmlC",
                "sad": "spotify:playlist:37i9dQZF1DX7qK8ma5wgG1",
                "party": "spotify:playlist:37i9dQZF1DXaXB8fQg7xif",
                "sleep": "spotify:playlist:37i9dQZF1DWZd79rJ6a7lp",
            ],
            genrePlaylists: [String: String] = [
                "indie folk": "spotify:playlist:37i9dQZF1DX2sUQwD7tbmL",
                "indie rock": "spotify:playlist:37i9dQZF1DX2Nc3B70tvx0",
                "hip hop": "spotify:playlist:37i9dQZF1DX0XUsuxWHRQd",
                "rock": "spotify:playlist:37i9dQZF1DXcF6B6QPhFDv",
                "pop": "spotify:playlist:37i9dQZF1DXcBWIGoYBM5M",
                "jazz": "spotify:playlist:37i9dQZF1DXbITWG1ZJKYt",
                "classical": "spotify:playlist:37i9dQZF1DWWEJlAGA9gs0",
                "country": "spotify:playlist:37i9dQZF1DX1lVhptIYRda",
                "electronic": "spotify:playlist:37i9dQZF1DX4dyzvuaRJ0n",
                "r&b": "spotify:playlist:37i9dQZF1DX4SBhb3fqCJd",
            ]
        ) {
            self.baseURL = baseURL
            self.cacheCapacity = cacheCapacity
            self.moodPlaylists = moodPlaylists
            self.genrePlaylists = genrePlaylists
        }
    }

    public typealias AccessTokenProvider = @Sendable () async throws -> String

    private let urlSession: URLSession
    private let config: Config
    private let accessTokenProvider: AccessTokenProvider

    private var cacheStorage: [String: String] = [:]
    private var cacheOrder: [String] = []

    public init(
        urlSession: URLSession = .shared,
        config: Config = Config(),
        accessTokenProvider: @escaping AccessTokenProvider
    ) {
        self.urlSession = urlSession
        self.config = config
        self.accessTokenProvider = accessTokenProvider
    }

    /// Decorate `prediction.slots.uri` with the resolved Spotify URI when
    /// the intent + slots support it. Returns the input unchanged when
    /// the prediction is not catalog-shaped (e.g. fast-path, WEBAPP_INTENT,
    /// CLARIFY, NO_INTENT).
    public func decorate(_ prediction: NluPrediction) async throws -> NluPrediction {
        var pred = prediction
        let intent = prediction.intent

        // CLARIFY / NO_INTENT / fast-path local commands: no catalog lookup.
        guard isCatalogIntent(intent) else { return pred }

        // Mood / genre route to curated playlist tables.
        if let mood = prediction.slots.mood?.lowercased(),
           let uri = config.moodPlaylists[mood] {
            pred.slots.uri = uri
            return pred
        }
        if let genre = prediction.slots.genre?.lowercased(),
           let uri = config.genrePlaylists[genre] {
            pred.slots.uri = uri
            return pred
        }

        // Pick the search type from whichever string slot the LLM populated.
        // Track-with-artist is the highest-precision catalog query; the
        // single-slot variants fall back to that slot's type.
        if let track = prediction.slots.track, !track.isEmpty {
            let query = [track, prediction.slots.artist].compactMap { $0 }.joined(separator: " ")
            pred.slots.uri = try await search(entityType: "track", query: query)
            return pred
        }
        if let artist = prediction.slots.artist, !artist.isEmpty {
            pred.slots.uri = try await search(entityType: "artist", query: artist)
            return pred
        }
        if let album = prediction.slots.album, !album.isEmpty {
            let query = [album, prediction.slots.artist].compactMap { $0 }.joined(separator: " ")
            pred.slots.uri = try await search(entityType: "album", query: query)
            return pred
        }
        if let playlist = prediction.slots.playlist, !playlist.isEmpty {
            pred.slots.uri = try await search(entityType: "playlist", query: playlist)
            return pred
        }
        if let podcast = prediction.slots.podcast, !podcast.isEmpty {
            pred.slots.uri = try await search(entityType: "show", query: podcast)
            return pred
        }
        if let episode = prediction.slots.episode, !episode.isEmpty {
            pred.slots.uri = try await search(entityType: "episode", query: episode)
            return pred
        }
        return pred
    }

    private func isCatalogIntent(_ intent: String) -> Bool {
        switch intent {
        case "PLAY", "ADD_TO_QUEUE", "ADD_TO_COLLECTION", "FOLLOW", "SHOW", "SEARCH":
            return true
        default:
            return false
        }
    }

    /// Run a Spotify Search query and return the top match's URI.
    func search(entityType: String, query: String) async throws -> String {
        let normalized = query.lowercased().trimmingCharacters(in: .whitespaces)
        let cacheKey = "\(entityType):\(normalized)"
        if let cached = cacheGet(cacheKey) { return cached }

        let token = try await accessTokenProvider()
        guard !token.isEmpty else { throw ResolveError.authMissing }

        var comps = URLComponents(url: config.baseURL.appendingPathComponent("search"), resolvingAgainstBaseURL: false)!
        comps.queryItems = [
            URLQueryItem(name: "q", value: query),
            URLQueryItem(name: "type", value: entityType),
            URLQueryItem(name: "limit", value: "5"),
        ]
        guard let url = comps.url else { throw ResolveError.invalidResponse(reason: "bad url") }

        var req = URLRequest(url: url)
        req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        let (data, response) = try await urlSession.data(for: req)
        guard let http = response as? HTTPURLResponse else {
            throw ResolveError.invalidResponse(reason: "no http status")
        }
        guard (200..<300).contains(http.statusCode) else {
            let body = String(data: data, encoding: .utf8) ?? "<binary>"
            throw ResolveError.httpError(statusCode: http.statusCode, body: body)
        }
        guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw ResolveError.invalidResponse(reason: "not json")
        }
        let pluralKey: String
        switch entityType {
        case "artist": pluralKey = "artists"
        case "track": pluralKey = "tracks"
        case "album": pluralKey = "albums"
        case "playlist": pluralKey = "playlists"
        case "show": pluralKey = "shows"
        case "episode": pluralKey = "episodes"
        default: pluralKey = entityType + "s"
        }
        guard let container = json[pluralKey] as? [String: Any],
              let items = container["items"] as? [[String: Any]]
        else {
            throw ResolveError.invalidResponse(reason: "missing \(pluralKey).items")
        }
        guard let top = items.first, let uri = top["uri"] as? String else {
            throw ResolveError.noMatch
        }
        cachePut(cacheKey, uri)
        return uri
    }

    private func cacheGet(_ key: String) -> String? {
        guard let uri = cacheStorage[key] else { return nil }
        if let idx = cacheOrder.firstIndex(of: key) {
            cacheOrder.remove(at: idx)
            cacheOrder.append(key)
        }
        return uri
    }

    private func cachePut(_ key: String, _ value: String) {
        if cacheStorage[key] != nil {
            if let idx = cacheOrder.firstIndex(of: key) {
                cacheOrder.remove(at: idx)
            }
        }
        cacheStorage[key] = value
        cacheOrder.append(key)
        while cacheOrder.count > config.cacheCapacity {
            let evict = cacheOrder.removeFirst()
            cacheStorage.removeValue(forKey: evict)
        }
    }
}
