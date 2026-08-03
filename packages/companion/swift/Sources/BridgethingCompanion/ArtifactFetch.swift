import Foundation

#if canImport(CryptoKit)
    import CryptoKit
#endif
#if canImport(FoundationNetworking)
    import FoundationNetworking
#endif

public struct ArtifactDigest: Decodable, Sendable, Equatable {
    public let size: UInt64
    public let sha256: String

    public init(size: UInt64, sha256: String) {
        self.size = size
        self.sha256 = sha256
    }
}

enum ArtifactFetchError: Error, CustomStringConvertible, LocalizedError {
    case httpStatus(Int)
    case digestMismatch(asset: String, field: String)
    case downloadIncomplete
    case cryptoUnavailable

    var description: String {
        switch self {
        case let .httpStatus(code): "fetch returned HTTP \(code)"
        case let .digestMismatch(asset, field): "\(asset) \(field) does not match the manifest; refusing to install"
        case .downloadIncomplete: "download finished without producing a file"
        case .cryptoUnavailable: "CryptoKit unavailable on this platform"
        }
    }

    var errorDescription: String? { description }
}

struct ArtifactFetcher: Sendable {
    var allowsExpensiveNetworkAccess: Bool = true

    func json<T: Decodable>(_: T.Type = T.self, from url: URL) async throws -> T {
        var req = URLRequest(url: url)
        req.cachePolicy = .reloadIgnoringLocalCacheData
        req.timeoutInterval = 30
        req.allowsExpensiveNetworkAccess = allowsExpensiveNetworkAccess
        let (data, response) = try await URLSession.shared.data(for: req)
        if let http = response as? HTTPURLResponse, !(200 ..< 300).contains(http.statusCode) {
            throw ArtifactFetchError.httpStatus(http.statusCode)
        }
        return try JSONDecoder().decode(T.self, from: data)
    }

    func downloadIfNeeded(
        url: URL,
        into directory: URL,
        filename: String,
        asset: String,
        expected: ArtifactDigest?,
        onProgress: (@Sendable (_ received: UInt64, _ total: UInt64) -> Void)? = nil
    ) async throws -> URL {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let cacheName = expected.map { "\(filename)-\($0.sha256)" } ?? filename
        let target = directory.appendingPathComponent(cacheName)
        if let attrs = try? FileManager.default.attributesOfItem(atPath: target.path),
           let size = (attrs[.size] as? NSNumber)?.uint64Value {
            if let expected {
                if size == expected.size { return target }
            } else if size > 0 {
                return target
            }
            try? FileManager.default.removeItem(at: target)
        }

        let downloader = ProgressDownloader(
            allowsExpensiveNetworkAccess: allowsExpensiveNetworkAccess,
            onProgress: onProgress ?? { _, _ in }
        )
        let (tmp, response) = try await downloader.download(url: url)
        if let http = response as? HTTPURLResponse, !(200 ..< 300).contains(http.statusCode) {
            try? FileManager.default.removeItem(at: tmp)
            throw ArtifactFetchError.httpStatus(http.statusCode)
        }
        if let expected {
            let size = (try FileManager.default.attributesOfItem(atPath: tmp.path)[.size] as? NSNumber)?.uint64Value ?? 0
            guard size == expected.size else {
                try? FileManager.default.removeItem(at: tmp)
                throw ArtifactFetchError.digestMismatch(asset: asset, field: "size")
            }
            let sha = try await Self.sha256(of: tmp)
            guard sha == expected.sha256 else {
                try? FileManager.default.removeItem(at: tmp)
                throw ArtifactFetchError.digestMismatch(asset: asset, field: "sha256")
            }
        }
        if FileManager.default.fileExists(atPath: target.path) {
            try FileManager.default.removeItem(at: target)
        }
        try FileManager.default.moveItem(at: tmp, to: target)
        return target
    }

    static func sha256(of url: URL) async throws -> String {
        #if canImport(CryptoKit)
            let fh = try FileHandle(forReadingFrom: url)
            defer { try? fh.close() }
            var h = SHA256()
            while true {
                let data = try fh.read(upToCount: 64 * 1024) ?? Data()
                if data.isEmpty { break }
                h.update(data: data)
            }
            return h.finalize().map { String(format: "%02x", $0) }.joined()
        #else
            throw ArtifactFetchError.cryptoUnavailable
        #endif
    }
}

private final class ProgressDownloader: NSObject, URLSessionDownloadDelegate, @unchecked Sendable {
    private let onProgress: @Sendable (UInt64, UInt64) -> Void
    private var continuation: CheckedContinuation<(URL, URLResponse), Error>?
    private var staged: URL?
    private let allowsExpensiveNetworkAccess: Bool
    private lazy var session: URLSession = {
        let config = URLSessionConfiguration.default
        config.allowsExpensiveNetworkAccess = allowsExpensiveNetworkAccess
        config.allowsConstrainedNetworkAccess = allowsExpensiveNetworkAccess
        return URLSession(configuration: config, delegate: self, delegateQueue: nil)
    }()

    init(allowsExpensiveNetworkAccess: Bool, onProgress: @escaping @Sendable (UInt64, UInt64) -> Void) {
        self.allowsExpensiveNetworkAccess = allowsExpensiveNetworkAccess
        self.onProgress = onProgress
    }

    func download(url: URL) async throws -> (URL, URLResponse) {
        try await withCheckedThrowingContinuation { cont in
            continuation = cont
            session.downloadTask(with: url).resume()
        }
    }

    func urlSession(
        _: URLSession,
        downloadTask _: URLSessionDownloadTask,
        didWriteData _: Int64,
        totalBytesWritten: Int64,
        totalBytesExpectedToWrite: Int64
    ) {
        onProgress(UInt64(max(totalBytesWritten, 0)), UInt64(max(totalBytesExpectedToWrite, 0)))
    }

    func urlSession(_: URLSession, downloadTask _: URLSessionDownloadTask, didFinishDownloadingTo location: URL) {
        let dest = FileManager.default.temporaryDirectory.appendingPathComponent("artifact-dl-\(UUID().uuidString)")
        staged = (try? FileManager.default.moveItem(at: location, to: dest)) == nil ? nil : dest
    }

    func urlSession(_: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        defer { session.finishTasksAndInvalidate() }
        let cont = continuation
        continuation = nil
        if let error {
            cont?.resume(throwing: error)
        } else if let staged, let response = task.response {
            cont?.resume(returning: (staged, response))
        } else {
            cont?.resume(throwing: ArtifactFetchError.downloadIncomplete)
        }
    }
}
