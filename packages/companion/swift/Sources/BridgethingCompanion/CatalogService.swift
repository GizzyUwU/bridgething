import BridgethingGateway
import BridgethingSchema
import Foundation

#if canImport(CryptoKit)
    import CryptoKit
#endif
#if canImport(FoundationNetworking)
    import FoundationNetworking
#endif

public protocol CatalogStore: Sendable {
    func loadSources() async -> [URL]
    func saveSources(_ urls: [URL]) async
}

public protocol CatalogFetcher: Sendable {
    func fetchCatalog(_ url: URL) async throws -> Catalog
    func download(_ url: URL, to destination: URL) async throws
}

public protocol WebappInstaller: Sendable {
    func installWebapp(
        gateway: BridgethingGateway,
        deviceId: String,
        bundlePath: URL,
        provenance: String?
    ) async -> WebappInstallResult
}

extension OtaService: WebappInstaller {}

public struct CatalogAppListing: Encodable, Sendable, Equatable {
    public let app: CatalogApp
    public let sourceURL: URL
    public let newestCompatible: CatalogAppVersion?
    public let installedVersion: String?
    public let updateAvailable: Bool
    public let alsoAvailableFrom: [URL]

    private enum CodingKeys: String, CodingKey {
        case app
        case sourceURL = "sourceUrl"
        case newestCompatible
        case installedVersion
        case updateAvailable
        case alsoAvailableFrom
    }
}

public struct CatalogAppUpdate: Encodable, Sendable, Equatable {
    public let appId: String
    public let name: String
    public let installedVersion: String
    public let target: CatalogAppVersion
    public let sourceURL: URL

    private enum CodingKeys: String, CodingKey {
        case appId
        case name
        case installedVersion
        case target
        case sourceURL = "sourceUrl"
    }
}

public struct CatalogPollConfig: Sendable, Equatable {
    public var intervalSeconds: TimeInterval
    public var autoInstall: Bool

    public init(intervalSeconds: TimeInterval = 21600, autoInstall: Bool = false) {
        self.intervalSeconds = intervalSeconds
        self.autoInstall = autoInstall
    }
}

public enum CatalogEvent: Sendable, Equatable {
    case refreshed(sourceCount: Int, appCount: Int)
    case sourceFailed(url: URL, reason: String)
    case updateAvailable(deviceId: String, update: CatalogAppUpdate)
    case installed(deviceId: String, appId: String, version: String)
    case installFailed(deviceId: String, appId: String, reason: String)
}

public actor CatalogService {
    private let store: any CatalogStore
    private let fetcher: any CatalogFetcher
    private let installer: any WebappInstaller
    private let officialURL: URL

    private var attachedGateway: BridgethingGateway?
    private var sourceURLs: [URL] = []
    private var deviceMeta: [String: BridgeThingMeta] = [:]
    private var catalogs: [URL: Catalog] = [:]
    private var loaded = false

    private var metaTask: Task<Void, Never>?
    private var pollTask: Task<Void, Never>?
    private var pollConfig: CatalogPollConfig?

    private let eventContinuation: AsyncStream<CatalogEvent>.Continuation
    public nonisolated let events: AsyncStream<CatalogEvent>

    public init(
        installer: any WebappInstaller,
        store: any CatalogStore = FileCatalogStore(),
        fetcher: any CatalogFetcher = URLSessionCatalogFetcher(),
        officialCatalogURL: URL = URL(string: "https://apps.bridgething.com/catalog.json")!
    ) {
        self.installer = installer
        self.store = store
        self.fetcher = fetcher
        officialURL = officialCatalogURL
        let (stream, continuation) = AsyncStream.makeStream(of: CatalogEvent.self)
        events = stream
        eventContinuation = continuation
    }

    public func start(gateway: BridgethingGateway) async {
        attachedGateway = gateway
        await loadStateIfNeeded()
        metaTask?.cancel()
        metaTask = Task { [weak self] in
            for await event in gateway.events {
                guard case let .message(deviceId, msg) = event,
                      case let .version(meta) = msg.data
                else { continue }
                guard let self else { return }
                await recordMeta(deviceId: deviceId, meta: meta)
            }
        }
    }

    public func stop() async {
        metaTask?.cancel()
        metaTask = nil
        pollTask?.cancel()
        pollTask = nil
        attachedGateway = nil
        deviceMeta.removeAll()
    }

    // MARK: - sources

    public func sources() async -> [URL] {
        await loadStateIfNeeded()
        return sourceURLs
    }

    public func addSource(_ url: URL) async {
        await loadStateIfNeeded()
        guard !sourceURLs.contains(url) else { return }
        sourceURLs.append(url)
        await store.saveSources(sourceURLs)
    }

    public func removeSource(_ url: URL) async {
        await loadStateIfNeeded()
        let before = sourceURLs.count
        sourceURLs.removeAll { $0 == url }
        if sourceURLs.count != before {
            catalogs.removeValue(forKey: url)
            await store.saveSources(sourceURLs)
        }
    }

    public func pinnedSource(deviceId: String, appId: String) async -> URL? {
        let installed = await installedApps(deviceId: deviceId)
        return Self.pins(from: installed)[appId]
    }

    // MARK: - browse

    public func refresh() async {
        await loadStateIfNeeded()
        for url in sourceURLs {
            do {
                catalogs[url] = try await fetcher.fetchCatalog(url)
            } catch {
                eventContinuation.yield(.sourceFailed(url: url, reason: String(describing: error)))
            }
        }
        let appCount = catalogs.values.reduce(0) { $0 + $1.apps.count }
        eventContinuation.yield(.refreshed(sourceCount: catalogs.count, appCount: appCount))
    }

    public func availableApps(deviceId: String) async -> [CatalogAppListing] {
        await loadStateIfNeeded()
        let installed = await installedApps(deviceId: deviceId)
        let deviceLib = deviceMeta[deviceId]?.libbridgethingVersion
        return Self.aggregate(
            orderedCatalogs: orderedCatalogs(),
            installed: installed,
            deviceLibVersion: deviceLib
        )
    }

    // MARK: - install

    @discardableResult
    public func install(
        deviceId: String,
        app: CatalogApp,
        version: CatalogAppVersion,
        sourceURL: URL
    ) async -> WebappInstallResult {
        guard let gateway = attachedGateway else {
            let reason = "gateway not attached"
            eventContinuation.yield(.installFailed(deviceId: deviceId, appId: app.id, reason: reason))
            return .failed(reason: reason)
        }
        let bundle: URL
        do {
            bundle = try await downloadVerified(version: version, appId: app.id)
        } catch {
            let reason = String(describing: error)
            eventContinuation.yield(.installFailed(deviceId: deviceId, appId: app.id, reason: reason))
            return .failed(reason: reason)
        }

        let result = await installer.installWebapp(
            gateway: gateway,
            deviceId: deviceId,
            bundlePath: bundle,
            provenance: sourceURL.absoluteString
        )
        try? FileManager.default.removeItem(at: bundle)
        switch result {
        case let .installed(info):
            eventContinuation.yield(.installed(deviceId: deviceId, appId: app.id, version: info.version))
        case let .failed(reason):
            eventContinuation.yield(.installFailed(deviceId: deviceId, appId: app.id, reason: reason))
        }
        return result
    }

    @discardableResult
    public func install(
        deviceId: String,
        appId: String,
        version: String,
        sourceURL: URL
    ) async -> WebappInstallResult {
        await loadStateIfNeeded()
        guard let app = catalogs[sourceURL]?.apps.first(where: { $0.id == appId }),
              let ver = app.versions.first(where: { $0.version == version })
        else {
            let reason = "app \(appId)@\(version) not found in \(sourceURL); refresh first"
            eventContinuation.yield(.installFailed(deviceId: deviceId, appId: appId, reason: reason))
            return .failed(reason: reason)
        }
        return await install(deviceId: deviceId, app: app, version: ver, sourceURL: sourceURL)
    }

    // MARK: - updates

    public func checkForUpdates(deviceId: String) async -> [CatalogAppUpdate] {
        await loadStateIfNeeded()
        let installed = await installedApps(deviceId: deviceId)
        let deviceLib = deviceMeta[deviceId]?.libbridgethingVersion
        return Self.updates(
            catalogs: catalogs,
            installed: installed,
            deviceLibVersion: deviceLib
        )
    }

    public func setPollConfig(_ config: CatalogPollConfig?) {
        pollConfig = config
        pollTask?.cancel()
        pollTask = nil
        guard let config else { return }
        pollTask = Task { [weak self] in
            await self?.runPollLoop(config: config)
        }
    }

    public func pollNow() async {
        guard let config = pollConfig else { return }
        await pollOnce(config: config)
    }

    private func runPollLoop(config: CatalogPollConfig) async {
        while !Task.isCancelled {
            await pollOnce(config: config)
            let nanos = UInt64(max(config.intervalSeconds, 60) * 1_000_000_000)
            try? await Task.sleep(nanoseconds: nanos)
        }
    }

    private func pollOnce(config: CatalogPollConfig) async {
        await refresh()
        let deviceIds = Array(deviceMeta.keys)
        for deviceId in deviceIds {
            let updates = await checkForUpdates(deviceId: deviceId)
            for update in updates {
                eventContinuation.yield(.updateAvailable(deviceId: deviceId, update: update))
                guard config.autoInstall else { continue }
                guard let app = catalogs[update.sourceURL]?.apps.first(where: { $0.id == update.appId }) else { continue }
                await install(deviceId: deviceId, app: app, version: update.target, sourceURL: update.sourceURL)
            }
        }
    }

    // MARK: - pure aggregation logic

    static func pins(from installed: [WebappInfo]) -> [String: URL] {
        var out: [String: URL] = [:]
        for info in installed {
            guard let raw = info.provenance, let url = URL(string: raw) else { continue }
            out[info.id.uuidString.lowercased()] = url
        }
        return out
    }

    static func aggregate(
        orderedCatalogs: [(url: URL, catalog: Catalog)],
        installed: [WebappInfo],
        deviceLibVersion: String?
    ) -> [CatalogAppListing] {
        let installedById = Dictionary(installed.map { ($0.id.uuidString.lowercased(), $0) }, uniquingKeysWith: { a, _ in a })
        let pins = pins(from: installed)

        var offerings: [String: [(url: URL, app: CatalogApp)]] = [:]
        var order: [String] = []
        for (url, catalog) in orderedCatalogs {
            for app in catalog.apps {
                if offerings[app.id] == nil { order.append(app.id) }
                offerings[app.id, default: []].append((url, app))
            }
        }

        var listings: [CatalogAppListing] = []
        for id in order {
            guard let offers = offerings[id], !offers.isEmpty else { continue }
            let pinned = pins[id]
            let primary = offers.first(where: { $0.url == pinned }) ?? offers[0]
            let alsoFrom = offers.map(\.url).filter { $0 != primary.url }

            let newest = newestCompatible(primary.app, deviceLibVersion: deviceLibVersion)
            let installedVersion = installedById[id]?.version
            let updateAvailable = installedVersion != nil
                && newest != nil
                && newest!.version != installedVersion

            listings.append(CatalogAppListing(
                app: primary.app,
                sourceURL: primary.url,
                newestCompatible: newest,
                installedVersion: installedVersion,
                updateAvailable: updateAvailable,
                alsoAvailableFrom: alsoFrom
            ))
        }
        return listings.sorted { ($0.app.name, $0.app.id) < ($1.app.name, $1.app.id) }
    }

    static func updates(
        catalogs: [URL: Catalog],
        installed: [WebappInfo],
        deviceLibVersion: String?
    ) -> [CatalogAppUpdate] {
        let pins = pins(from: installed)
        var out: [CatalogAppUpdate] = []
        for info in installed where info.source == .installed && info.role == .standard {
            let id = info.id.uuidString.lowercased()
            guard let sourceURL = pins[id],
                  let app = catalogs[sourceURL]?.apps.first(where: { $0.id == id }),
                  let newest = newestCompatible(app, deviceLibVersion: deviceLibVersion),
                  newest.version != info.version
            else { continue }
            out.append(CatalogAppUpdate(
                appId: id,
                name: app.name,
                installedVersion: info.version,
                target: newest,
                sourceURL: sourceURL
            ))
        }
        return out.sorted { ($0.name, $0.appId) < ($1.name, $1.appId) }
    }

    static func releasedAtInstant(_ raw: String) -> Date? {
        let fractional = ISO8601DateFormatter()
        fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let parsed = fractional.date(from: raw) { return parsed }
        let plain = ISO8601DateFormatter()
        plain.formatOptions = [.withInternetDateTime]
        return plain.date(from: raw)
    }

    static func sortedByReleasedAt(_ versions: [CatalogAppVersion]) -> [CatalogAppVersion] {
        versions.enumerated()
            .map { (offset: $0.offset, element: $0.element, at: releasedAtInstant($0.element.releasedAt)) }
            .sorted { a, b in
                switch (a.at, b.at) {
                case let (l?, r?):
                    return l == r ? a.offset < b.offset : l > r
                case (nil, nil):
                    return a.offset < b.offset
                case (nil, _):
                    return false
                case (_, nil):
                    return true
                }
            }
            .map(\.element)
    }

    static func newestCompatible(_ app: CatalogApp, deviceLibVersion: String?) -> CatalogAppVersion? {
        let ordered = sortedByReleasedAt(app.versions)
        guard let deviceLib = deviceLibVersion else { return ordered.first }
        return ordered.first {
            SemverCompat.satisfies(deviceVersion: deviceLib, minimum: $0.minLibbridgethingVersion)
        }
    }

    // MARK: - internals

    private func orderedCatalogs() -> [(url: URL, catalog: Catalog)] {
        sourceURLs.compactMap { url in catalogs[url].map { (url, $0) } }
    }

    private func loadStateIfNeeded() async {
        guard !loaded else { return }
        var sources = await store.loadSources()
        if sources.isEmpty {
            sources = [officialURL]
            await store.saveSources(sources)
        }
        sourceURLs = sources
        loaded = true
    }

    private func recordMeta(deviceId: String, meta: BridgeThingMeta) {
        deviceMeta[deviceId] = meta
    }

    private func installedApps(deviceId: String) async -> [WebappInfo] {
        guard let gateway = attachedGateway else { return [] }
        guard let result = try? await gateway.webapp.list(deviceId: deviceId),
              case let .ok(list) = result
        else { return [] }
        return list.webapps
    }

    private func downloadVerified(version: CatalogAppVersion, appId: String) async throws -> URL {
        guard let url = URL(string: version.download.url) else {
            throw CatalogServiceError.badDownloadURL(version.download.url)
        }
        let dir = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first
            ?? URL(fileURLWithPath: NSTemporaryDirectory())
        let cacheDir = dir.appendingPathComponent("bridgething-catalog", isDirectory: true)
        try FileManager.default.createDirectory(at: cacheDir, withIntermediateDirectories: true)
        let dest = cacheDir.appendingPathComponent("\(appId)-\(version.version).zip")
        try? FileManager.default.removeItem(at: dest)

        try await fetcher.download(url, to: dest)

        let size = (try? FileManager.default.attributesOfItem(atPath: dest.path)[.size] as? NSNumber)??.intValue ?? -1
        if size != version.download.size {
            try? FileManager.default.removeItem(at: dest)
            throw CatalogServiceError.sizeMismatch(expected: version.download.size, got: size)
        }
        let digest = try hashFile(dest)
        if digest != version.download.sha256.lowercased() {
            try? FileManager.default.removeItem(at: dest)
            throw CatalogServiceError.sha256Mismatch(expected: version.download.sha256, got: digest)
        }
        return dest
    }

    private func hashFile(_ url: URL) throws -> String {
        #if canImport(CryptoKit)
            let handle = try FileHandle(forReadingFrom: url)
            defer { try? handle.close() }
            var hasher = SHA256()
            while true {
                let data = try handle.read(upToCount: 64 * 1024) ?? Data()
                if data.isEmpty { break }
                hasher.update(data: data)
            }
            return hasher.finalize().map { String(format: "%02x", $0) }.joined()
        #else
            throw CatalogServiceError.cryptoUnavailable
        #endif
    }
}

enum CatalogServiceError: Error, CustomStringConvertible {
    case badDownloadURL(String)
    case sizeMismatch(expected: Int, got: Int)
    case sha256Mismatch(expected: String, got: String)
    case cryptoUnavailable

    var description: String {
        switch self {
        case let .badDownloadURL(url): "invalid download URL '\(url)'"
        case let .sizeMismatch(expected, got): "download size \(got) != catalog size \(expected)"
        case let .sha256Mismatch(expected, got): "download sha256 \(got) != catalog sha256 \(expected)"
        case .cryptoUnavailable: "CryptoKit unavailable on this platform"
        }
    }
}

public actor FileCatalogStore: CatalogStore {
    private let sourcesURL: URL

    public init(directory: URL? = nil) {
        let base = directory
            ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first?
            .appendingPathComponent("bridgething-catalog", isDirectory: true)
            ?? URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("bridgething-catalog")
        try? FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
        sourcesURL = base.appendingPathComponent("sources.json")
    }

    public func loadSources() -> [URL] {
        guard let data = try? Data(contentsOf: sourcesURL),
              let strings = try? JSONDecoder().decode([String].self, from: data)
        else { return [] }
        return strings.compactMap(URL.init(string:))
    }

    public func saveSources(_ urls: [URL]) {
        let strings = urls.map(\.absoluteString)
        if let data = try? JSONEncoder().encode(strings) { try? data.write(to: sourcesURL) }
    }
}

public actor InMemoryCatalogStore: CatalogStore {
    private var sources: [URL]

    public init(sources: [URL] = []) {
        self.sources = sources
    }

    public func loadSources() -> [URL] { sources }
    public func saveSources(_ urls: [URL]) { sources = urls }
}

public final class URLSessionCatalogFetcher: CatalogFetcher {
    public init() {}

    public func fetchCatalog(_ url: URL) async throws -> Catalog {
        var req = URLRequest(url: url)
        req.cachePolicy = .reloadIgnoringLocalCacheData
        req.timeoutInterval = 30
        let (data, response) = try await URLSession.shared.data(for: req)
        if let http = response as? HTTPURLResponse, !(200 ..< 300).contains(http.statusCode) {
            throw CatalogFetchError.httpStatus(http.statusCode)
        }
        return try JSONDecoder().decode(Catalog.self, from: data)
    }

    public func download(_ url: URL, to destination: URL) async throws {
        let (tmp, response) = try await URLSession.shared.download(from: url)
        if let http = response as? HTTPURLResponse, !(200 ..< 300).contains(http.statusCode) {
            try? FileManager.default.removeItem(at: tmp)
            throw CatalogFetchError.httpStatus(http.statusCode)
        }
        if FileManager.default.fileExists(atPath: destination.path) {
            try FileManager.default.removeItem(at: destination)
        }
        try FileManager.default.moveItem(at: tmp, to: destination)
    }
}

enum CatalogFetchError: Error, CustomStringConvertible {
    case httpStatus(Int)
    var description: String {
        switch self {
        case let .httpStatus(code): "catalog fetch returned HTTP \(code)"
        }
    }
}
