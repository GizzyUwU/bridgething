import CryptoKit
import Foundation
import Spotiny

final class CassetteExecutor: SpotinyHTTPExecutor, @unchecked Sendable {
    struct Entry: Codable {
        let request: SpotinyHTTPRequest
        let response: SpotinyHTTPResponse
    }

    enum CassetteError: Error {
        case rateLimited(String)
        case noCassetteNoToken(String)
    }

    let dir: URL
    let live: any SpotinyHTTPExecutor
    let refresh: Bool
    let allowLive: Bool

    init(dir: URL, live: any SpotinyHTTPExecutor = SharedHTTPClientExecutor(), refresh: Bool, allowLive: Bool) {
        self.dir = dir
        self.live = live
        self.refresh = refresh
        self.allowLive = allowLive
    }

    func execute(_ request: SpotinyHTTPRequest) async throws -> SpotinyHTTPResponse {
        let file = dir.appendingPathComponent("\(Self.key(for: request)).json")

        if !refresh, let data = try? Data(contentsOf: file),
           let entry = try? JSONDecoder().decode(Entry.self, from: data) {
            return entry.response
        }

        guard allowLive else {
            throw CassetteError.noCassetteNoToken(request.url)
        }

        let response = try await live.execute(request)

        if response.status == 429 {
            throw CassetteError.rateLimited(request.url)
        }

        if (200 ..< 300).contains(response.status) || response.status == 404 {
            let entry = Entry(request: Self.scrub(request), response: response)
            try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            if let data = try? JSONEncoder().encode(entry) {
                try? data.write(to: file, options: .atomic)
            }
        }

        return response
    }

    private static func scrub(_ req: SpotinyHTTPRequest) -> SpotinyHTTPRequest {
        var scrubbed = req
        scrubbed.headers["Authorization"] = nil
        return scrubbed
    }

    static func key(for req: SpotinyHTTPRequest) -> String {
        var hasher = SHA256()
        hasher.update(data: Data(req.method.utf8))
        hasher.update(data: Data("\n".utf8))
        hasher.update(data: Data(req.url.utf8))
        if let body = req.body {
            hasher.update(data: Data("\n".utf8))
            hasher.update(data: body)
        }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }
}
