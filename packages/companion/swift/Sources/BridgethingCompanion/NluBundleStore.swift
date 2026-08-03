import Foundation
import ZIPFoundation

public struct NluBundleArtifact: Decodable, Sendable, Equatable {
    public let url: URL
    public let size: UInt64
    public let sha256: String

    var digest: ArtifactDigest { ArtifactDigest(size: size, sha256: sha256) }
}

public struct NluBundleManifest: Decodable, Sendable, Equatable {
    public let version: String
    public let updatedAt: String
    public let ios: NluBundleArtifact?
    public let android: NluBundleArtifact?

    private enum CodingKeys: String, CodingKey {
        case version
        case updatedAt = "updated_at"
        case ios
        case android
    }
}

public enum NluBundleState: Sendable, Equatable {
    case absent
    case downloading(received: UInt64, total: UInt64)
    case ready(version: String)
    case failed(reason: String)
}

protocol NluBundleTransport: Sendable {
    func manifest(from url: URL) async throws -> NluBundleManifest
    func download(
        _ artifact: NluBundleArtifact,
        into directory: URL,
        onProgress: @escaping @Sendable (UInt64, UInt64) -> Void
    ) async throws -> URL
}

struct NluBundleHttpTransport: NluBundleTransport {
    private let fetcher = ArtifactFetcher(allowsExpensiveNetworkAccess: false)

    func manifest(from url: URL) async throws -> NluBundleManifest {
        try await fetcher.json(NluBundleManifest.self, from: url)
    }

    func download(
        _ artifact: NluBundleArtifact,
        into directory: URL,
        onProgress: @escaping @Sendable (UInt64, UInt64) -> Void
    ) async throws -> URL {
        try await fetcher.downloadIfNeeded(
            url: artifact.url,
            into: directory,
            filename: "bundle.zip",
            asset: "nlu bundle",
            expected: artifact.digest,
            onProgress: onProgress
        )
    }
}

enum NluBundleStoreError: Error, CustomStringConvertible {
    case noArtifactForPlatform
    case malformedArchive(missing: String)

    var description: String {
        switch self {
        case .noArtifactForPlatform: "the nlu manifest carries no bundle for this platform"
        case let .malformedArchive(missing): "nlu archive is missing \(missing)"
        }
    }
}

public actor NluBundleStore {
    public typealias Validator = @Sendable (URL) async throws -> Void

    public struct Config: Sendable {
        public var rootURL: URL
        public var channel: String
        public var storageDirectory: URL?

        public init(
            rootURL: URL = URL(string: "https://ota.bridgething.com")!,
            channel: String = "stable",
            storageDirectory: URL? = nil
        ) {
            self.rootURL = rootURL
            self.channel = channel
            self.storageDirectory = storageDirectory
        }
    }

    private let config: Config
    private let transport: any NluBundleTransport
    private let validator: Validator
    private let root: URL

    private var enabled: Bool
    private var inFlight: Task<Void, Never>?
    private var stateValue: NluBundleState = .absent

    private nonisolated let stateContinuation: AsyncStream<NluBundleState>.Continuation

    public nonisolated let stateChanges: AsyncStream<NluBundleState>

    public init(
        config: Config = Config(),
        enabled: Bool = true,
        validator: @escaping Validator
    ) {
        self.init(config: config, enabled: enabled, transport: NluBundleHttpTransport(), validator: validator)
    }

    init(
        config: Config,
        enabled: Bool,
        transport: any NluBundleTransport,
        validator: @escaping Validator
    ) {
        self.config = config
        self.transport = transport
        self.validator = validator
        self.enabled = enabled
        let (stream, continuation) = AsyncStream.makeStream(of: NluBundleState.self)
        stateChanges = stream
        stateContinuation = continuation
        let base = config.storageDirectory
            ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? URL(fileURLWithPath: NSTemporaryDirectory())
        root = base.appendingPathComponent("bridgething-nlu", isDirectory: true)
        if let version = Self.readCurrent(root: root), Self.bundleLooksComplete(root.appendingPathComponent(version)) {
            stateValue = .ready(version: version)
        }
    }

    public var state: NluBundleState { stateValue }

    public var liveBundle: URL? {
        guard case let .ready(version) = stateValue else { return nil }
        return root.appendingPathComponent(version, isDirectory: true)
    }

    public func setEnabled(_ value: Bool) {
        guard value != enabled else { return }
        enabled = value
        if value {
            Task { [weak self] in await self?.ensure() }
        } else {
            inFlight?.cancel()
            inFlight = nil
            try? FileManager.default.removeItem(at: root)
            publish(.absent)
        }
    }

    public func ensure() async {
        guard enabled else { return }
        if let inFlight { return await inFlight.value }
        let task = Task { [weak self] in await self?.run() ?? () }
        inFlight = task
        await task.value
        inFlight = nil
    }

    private func publish(_ state: NluBundleState) {
        stateValue = state
        stateContinuation.yield(state)
    }

    private func publishProgress(received: UInt64, total: UInt64) {
        guard case .downloading = stateValue else { return }
        publish(.downloading(received: received, total: total))
    }

    private func run() async {
        do {
            let manifest = try await transport.manifest(
                from: config.rootURL
                    .appendingPathComponent("nlu")
                    .appendingPathComponent(config.channel)
                    .appendingPathComponent("manifest.json")
            )
            guard let artifact = manifest.ios else { throw NluBundleStoreError.noArtifactForPlatform }
            let installed = Self.readCurrent(root: root)
            if installed == manifest.version, Self.bundleLooksComplete(root.appendingPathComponent(manifest.version)) {
                publish(.ready(version: manifest.version))
                return
            }
            try await install(version: manifest.version, artifact: artifact)
        } catch is CancellationError {
            return
        } catch {
            if let installed = Self.readCurrent(root: root) {
                publish(.ready(version: installed))
            } else {
                publish(.failed(reason: String(describing: error)))
            }
        }
    }

    private func install(version: String, artifact: NluBundleArtifact) async throws {
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let downloads = root.appendingPathComponent("downloads", isDirectory: true)
        let staging = root.appendingPathComponent("staging-\(UUID().uuidString)", isDirectory: true)

        publish(.downloading(received: 0, total: artifact.size))
        let total = artifact.size
        let archive = try await transport.download(artifact, into: downloads) { [weak self] received, reported in
            Task { await self?.publishProgress(received: received, total: total > 0 ? total : reported) }
        }

        do {
            try FileManager.default.createDirectory(at: staging, withIntermediateDirectories: true)
            try FileManager.default.unzipItem(at: archive, to: staging)
            try Self.requireBundleShape(staging)
            try await validator(staging)
            try rotate(staging: staging, version: version)
        } catch {
            try? FileManager.default.removeItem(at: staging)
            throw error
        }

        try? FileManager.default.removeItem(at: downloads)
        publish(.ready(version: version))
    }

    private func rotate(staging: URL, version: String) throws {
        let live = root.appendingPathComponent(version, isDirectory: true)
        if FileManager.default.fileExists(atPath: live.path) {
            try FileManager.default.removeItem(at: live)
        }
        try FileManager.default.moveItem(at: staging, to: live)
        try Data(version.utf8).write(to: root.appendingPathComponent("current"), options: .atomic)
        pruneSuperseded(keeping: version)
    }

    private func pruneSuperseded(keeping version: String) {
        let entries = (try? FileManager.default.contentsOfDirectory(atPath: root.path)) ?? []
        for entry in entries where entry != version && entry != "current" {
            try? FileManager.default.removeItem(at: root.appendingPathComponent(entry))
        }
    }

    private static func readCurrent(root: URL) -> String? {
        guard let data = try? Data(contentsOf: root.appendingPathComponent("current")),
              let version = String(data: data, encoding: .utf8), !version.isEmpty else { return nil }
        return version
    }

    private static func bundleLooksComplete(_ dir: URL) -> Bool {
        (try? requireBundleShape(dir)) != nil
    }

    @discardableResult
    private static func requireBundleShape(_ dir: URL) throws -> Bool {
        for entry in ["manifest.json", "tokenizer.json", "model.mlpackage"] {
            guard FileManager.default.fileExists(atPath: dir.appendingPathComponent(entry).path) else {
                throw NluBundleStoreError.malformedArchive(missing: entry)
            }
        }
        return true
    }
}
