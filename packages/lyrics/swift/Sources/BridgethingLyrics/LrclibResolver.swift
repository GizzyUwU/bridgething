import Foundation

#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// `LyricsResolver` backed by lrclib.net's signature lookup endpoint.
/// No auth, free, community-uploaded LRC. Coverage is strong on western
/// mainstream and weak on remixes / live / regional / very new releases.
public final class LrclibResolver: LyricsResolver, @unchecked Sendable {
    public let name: String = "lrclib"

    private let baseURL: URL
    private let userAgent: String
    private let session: URLSession

    public init(
        baseURL: URL = URL(string: "https://lrclib.net")!,
        userAgent: String = "bridgething/0.1 (+https://github.com/thinglabsoss/bridgething)",
        session: URLSession = .shared
    ) {
        self.baseURL = baseURL
        self.userAgent = userAgent
        self.session = session
    }

    public func lyrics(for track: TrackIdentity) async -> Lyrics? {
        guard let url = makeQueryURL(for: track) else { return nil }

        var request = URLRequest(url: url)
        request.setValue(userAgent, forHTTPHeaderField: "User-Agent")
        request.setValue("application/json", forHTTPHeaderField: "Accept")

        do {
            let (data, response) = try await session.data(for: request)
            guard let http = response as? HTTPURLResponse, http.statusCode == 200 else {
                return nil
            }
            let entry = try JSONDecoder().decode(LrclibEntry.self, from: data)
            return entry.toLyrics()
        } catch {
            return nil
        }
    }

    private func makeQueryURL(for track: TrackIdentity) -> URL? {
        var components = URLComponents(url: baseURL.appendingPathComponent("api/get"), resolvingAgainstBaseURL: false)
        var items: [URLQueryItem] = [
            URLQueryItem(name: "artist_name", value: track.artist),
            URLQueryItem(name: "track_name", value: track.track),
        ]
        if let album = track.album {
            items.append(URLQueryItem(name: "album_name", value: album))
        }
        if let durationMs = track.durationMs {
            items.append(URLQueryItem(name: "duration", value: String(durationMs / 1000)))
        }
        components?.queryItems = items
        return components?.url
    }
}

private struct LrclibEntry: Decodable {
    let plainLyrics: String?
    let syncedLyrics: String?
    let instrumental: Bool?

    func toLyrics() -> Lyrics? {
        if instrumental == true {
            return Lyrics(synced: nil, plain: nil, source: "lrclib")
        }
        let synced = syncedLyrics.flatMap { LRCParser.parse($0) }
        if synced == nil, (plainLyrics ?? "").isEmpty {
            return nil
        }
        return Lyrics(
            synced: synced?.isEmpty == true ? nil : synced,
            plain: plainLyrics?.isEmpty == true ? nil : plainLyrics,
            source: "lrclib"
        )
    }
}

/// Parses the LRC time-tagged format. Each line may carry one or more
/// `[mm:ss.xx]` timestamps followed by the line text; lines without any
/// timestamp are ignored. Multiple timestamps on a single line emit
/// multiple `LyricLine` entries with the same text.
enum LRCParser {
    static func parse(_ text: String) -> [LyricLine] {
        var out: [LyricLine] = []
        for rawLine in text.split(separator: "\n", omittingEmptySubsequences: false) {
            let line = String(rawLine)
            let (timestamps, body) = extractTimestamps(line)
            if timestamps.isEmpty { continue }
            for ms in timestamps {
                out.append(LyricLine(startMs: ms, text: body))
            }
        }
        return out.sorted(by: { $0.startMs < $1.startMs })
    }

    private static func extractTimestamps(_ line: String) -> (timestamps: [Int], body: String) {
        var stamps: [Int] = []
        var rest = Substring(line)
        while rest.hasPrefix("[") {
            guard let close = rest.firstIndex(of: "]") else { break }
            let inside = rest[rest.index(after: rest.startIndex)..<close]
            if let ms = parseTimestamp(String(inside)) {
                stamps.append(ms)
                rest = rest[rest.index(after: close)...]
            } else {
                break
            }
        }
        let body = String(rest).trimmingCharacters(in: .whitespaces)
        return (stamps, body)
    }

    private static func parseTimestamp(_ s: String) -> Int? {
        let parts = s.split(separator: ":")
        guard parts.count == 2 else { return nil }
        guard let minutes = Int(parts[0]) else { return nil }
        let secondParts = parts[1].split(separator: ".")
        guard let seconds = Int(secondParts[0]) else { return nil }
        var hundredths = 0
        if secondParts.count == 2 {
            let frac = secondParts[1]
            let normalized: String
            if frac.count == 2 {
                normalized = String(frac)
            } else if frac.count == 3 {
                normalized = String(frac.prefix(2))
            } else {
                normalized = String(frac).padding(toLength: 2, withPad: "0", startingAt: 0)
            }
            hundredths = Int(normalized) ?? 0
        }
        return (minutes * 60 + seconds) * 1000 + hundredths * 10
    }
}
