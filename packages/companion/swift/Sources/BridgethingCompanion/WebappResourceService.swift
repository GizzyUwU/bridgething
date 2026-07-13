import BridgethingGateway
import BridgethingSchema
import Foundation
#if canImport(CryptoKit)
    import CryptoKit
#endif

public enum WebappResourceError: Error, CustomStringConvertible {
    case notStarted
    case staleCacheMissing
    case shaMismatch(expected: String, got: String)
    case cryptoUnavailable
    case domain(WebappError)
    case wire(WireError)

    public var description: String {
        switch self {
        case .notStarted: "webapp resource service not started"
        case .staleCacheMissing: "daemon reported cache current but no cached file exists"
        case let .shaMismatch(expected, got): "resource sha256 mismatch: expected \(expected), got \(got)"
        case .cryptoUnavailable: "CryptoKit unavailable on this platform"
        case let .domain(err): "daemon rejected resource fetch: \(err)"
        case let .wire(err): "resource fetch protocol error: \(err)"
        }
    }
}

public actor WebappResourceService {
    public struct Resolved: Sendable {
        public let url: URL
        public let mime: String?
        public let sha256: String
    }

    private let receiver: TransferReceiver
    private let cacheDir: URL
    private var gateway: BridgethingGateway?

    init(receiver: TransferReceiver, cacheDirectory: URL? = nil) {
        self.receiver = receiver
        let base = cacheDirectory
            ?? FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first
            ?? URL(fileURLWithPath: NSTemporaryDirectory())
        cacheDir = base.appendingPathComponent("bridgething-webapp-resources", isDirectory: true)
    }

    func start(gateway: BridgethingGateway) {
        self.gateway = gateway
    }

    public func fetch(deviceId: String, webappId: UUID, kind: WebappResourceKind) async throws -> Resolved {
        guard let gateway else { throw WebappResourceError.notStarted }
        let cached = newestCached(webappId: webappId, kind: kind)
        let result = try await gateway.webapp.resource(
            deviceId: deviceId,
            WebappResource(id: webappId, kind: kind, have: cached?.sha256)
        )
        switch result {
        case let .ok(reply):
            guard let body = reply.body else {
                guard let cached else { throw WebappResourceError.staleCacheMissing }
                let mime = reply.mime ?? Self.mime(forExtension: cached.url.pathExtension)
                return Resolved(url: cached.url, mime: mime, sha256: reply.sha256)
            }
            let data: Data
            switch body {
            case let .inline(bytes):
                data = bytes
            case let .stream(ref):
                await receiver.register(deviceId: deviceId, ref: ref)
                data = try await receiver.collect(ref.id, timeout: .seconds(30))
            }
            if let err = shaError(data, reply.sha256) { throw err }
            let url = try write(webappId: webappId, kind: kind, sha256: reply.sha256, mime: reply.mime, data: data)
            return Resolved(url: url, mime: reply.mime, sha256: reply.sha256)
        case let .domain(err):
            throw WebappResourceError.domain(err)
        case let .protocolError(err):
            throw WebappResourceError.wire(err)
        }
    }

    private func newestCached(webappId: UUID, kind: WebappResourceKind) -> (url: URL, sha256: String)? {
        let prefix = "\(webappId.uuidString)__\(kind.rawValue)__"
        let fm = FileManager.default
        guard let entries = try? fm.contentsOfDirectory(
            at: cacheDir, includingPropertiesForKeys: [.contentModificationDateKey]
        ) else { return nil }
        let matches = entries.filter { $0.lastPathComponent.hasPrefix(prefix) }
        let newest = matches.max { a, b in
            let da = (try? a.resourceValues(forKeys: [.contentModificationDateKey]))?.contentModificationDate ?? .distantPast
            let db = (try? b.resourceValues(forKeys: [.contentModificationDateKey]))?.contentModificationDate ?? .distantPast
            return da < db
        }
        guard let newest else { return nil }
        let stem = newest.deletingPathExtension().lastPathComponent
        guard let sha = stem.components(separatedBy: "__").last, !sha.isEmpty else { return nil }
        return (cacheDir.appendingPathComponent(newest.lastPathComponent), sha)
    }

    private func write(webappId: UUID, kind: WebappResourceKind, sha256: String, mime: String?, data: Data) throws -> URL {
        let fm = FileManager.default
        try fm.createDirectory(at: cacheDir, withIntermediateDirectories: true)
        let name = "\(webappId.uuidString)__\(kind.rawValue)__\(sha256).\(Self.ext(for: mime))"
        let dest = cacheDir.appendingPathComponent(name)
        try data.write(to: dest, options: .atomic)
        let prefix = "\(webappId.uuidString)__\(kind.rawValue)__"
        if let entries = try? fm.contentsOfDirectory(at: cacheDir, includingPropertiesForKeys: nil) {
            for url in entries where url.lastPathComponent.hasPrefix(prefix) && url.lastPathComponent != name {
                try? fm.removeItem(at: url)
            }
        }
        return dest
    }

    private static func ext(for mime: String?) -> String {
        guard let mime = mime?.lowercased() else { return "bin" }
        if mime.contains("svg") { return "svg" }
        if mime.contains("png") { return "png" }
        if mime.contains("jpeg") || mime.contains("jpg") { return "jpeg" }
        if mime.contains("webp") { return "webp" }
        if mime.contains("gif") { return "gif" }
        if mime.contains("html") { return "html" }
        return "bin"
    }

    private static func mime(forExtension ext: String) -> String? {
        switch ext.lowercased() {
        case "svg": "image/svg+xml"
        case "png": "image/png"
        case "jpeg", "jpg": "image/jpeg"
        case "webp": "image/webp"
        case "gif": "image/gif"
        case "html": "text/html"
        default: nil
        }
    }

    private func shaError(_ data: Data, _ expected: String) -> WebappResourceError? {
        #if canImport(CryptoKit)
            let got = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
            guard got.caseInsensitiveCompare(expected) == .orderedSame else {
                return .shaMismatch(expected: expected, got: got)
            }
            return nil
        #else
            return .cryptoUnavailable
        #endif
    }
}
