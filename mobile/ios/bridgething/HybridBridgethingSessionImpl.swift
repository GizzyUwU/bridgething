import BridgethingCompanion
import BridgethingGateway
import BridgethingGlue
import BridgethingLyrics
import BridgethingSchema
import BridgethingSession
import CryptoKit
import Foundation
import NitroModules
import UIKit

/// `BridgethingSessionBackend` impl. JS owns preferences and reapplies them on bootstrap.
public final class HybridBridgethingSessionImpl: BridgethingSessionBackend, @unchecked Sendable {
    public typealias GlueFactory = @Sendable () -> any BridgethingGlue
    public typealias SignOutFn = @Sendable () -> Void
    public typealias HasCredentialsFn = @Sendable () -> Bool

    public struct ProviderRegistration: Sendable {
        public let id: String
        public let displayName: String
        public let available: Bool
        public let factory: GlueFactory
        public let signOut: SignOutFn
        public let hasCredentials: HasCredentialsFn

        public init(
            id: String,
            displayName: String,
            available: Bool,
            factory: @escaping GlueFactory,
            signOut: @escaping SignOutFn,
            hasCredentials: @escaping HasCredentialsFn = { false }
        ) {
            self.id = id
            self.displayName = displayName
            self.available = available
            self.factory = factory
            self.signOut = signOut
            self.hasCredentials = hasCredentials
        }
    }

    public static var registry: [ProviderRegistration] = []
    public static var hostInfo: HostInfo = .init(appName: "bridgething", appVersion: "0.0.0", osName: "iOS")
    public static var lyricsResolver: any LyricsResolver = LrclibResolver()
    public static var eaProtocolString: String = "com.bridgething.gateway"

    private let stateLock = NSLock()
    private var companion: BridgethingCompanion?
    private var eventsTask: Task<Void, Never>?
    private var authTask: Task<Void, Never>?
    private var otaEventsTask: Task<Void, Never>?
    private var peers: [String: BridgethingSessionPeer] = [:]
    private var lastNowPlaying: BridgethingNowPlaying?
    private var activeRegistration: ProviderRegistration?

    private var onProviderChanged: (@Sendable (BridgethingProviderInfo?) -> Void)?
    private var onAuthStateChanged: (@Sendable (BridgethingAuthState) -> Void)?
    private var onServiceHealthChanged: (@Sendable (BridgethingServiceHealth) -> Void)?
    private var onPeerConnected: (@Sendable (BridgethingSessionPeer) -> Void)?
    private var onPeerDisconnected: (@Sendable (String) -> Void)?
    private var onPeerLinkFailed: (@Sendable (BridgethingSessionPeer) -> Void)?
    private var onNowPlayingChanged: (@Sendable (BridgethingNowPlaying?) -> Void)?
    private var onAncsAuthStatusChanged: (@Sendable (BridgethingAncsAuthStatus) -> Void)?
    private var onLog: (@Sendable (String, String) -> Void)?
    private var onWebappsChanged: (@Sendable (String) -> Void)?
    private var onDeviceMetaChanged: (@Sendable (String, BridgethingDeviceMeta) -> Void)?
    private var onOtaEvent: (@Sendable (BridgethingOtaEvent) -> Void)?
    private var onDiagEntry: (@Sendable (BridgethingDiagEntry) -> Void)?
    private var diagTask: Task<Void, Never>?
    private var logStreamingDesired: Bool = false
    private var lastAuthState: BridgethingAuthState = .idleState()
    private var lastServiceHealth: BridgethingServiceHealth = toRNServiceHealth(.ok)

    public init() {}

    // MARK: - Lifecycle

    public func start() async throws {
        if stateLock.withLock({ self.companion != nil }) { return }
        let adapter = EAAccessoryAdapter(protocolString: Self.eaProtocolString)
        let host = Self.makeHostInfo()
        let companion = BridgethingCompanion(
            adapter: adapter,
            lyricsResolver: Self.lyricsResolver,
            host: host,
            capabilities: Self.loadCompanionCapabilityFlags()
        )
        stateLock.lock(); self.companion = companion; stateLock.unlock()

        await companion.setNowPlayingObserver { [weak self] np in
            self?.handleNowPlaying(np)
        }
        await companion.setAncsAuthStateObserver { [weak self] state in
            self?.emitAncsAuthStatus(toRNAncsAuthStatus(state))
        }
        if stateLock.withLock({ logStreamingDesired }) {
            await companion.setLogObserver { [weak self] level, message in
                self?.emitLog(level.rawValue, message)
            }
            await companion.setDeviceLogStreaming(true)
        }

        try await companion.start()

        let events = companion.gateway.events
        let task = Task { [weak self] in
            for await event in events {
                self?.handleGatewayEvent(event)
            }
        }
        let ota = await companion.ota
        let otaStream = ota.events
        let otaTask = Task { [weak self] in
            for await event in otaStream {
                self?.emitOtaEvent(toRNOtaEvent(event))
            }
        }
        let diagStream = DiagnosticsBuffer.shared.stream
        let diagTask = Task { [weak self] in
            for await record in diagStream {
                guard let self else { return }
                let cb = stateLock.withLock { onDiagEntry }
                cb?(Self.toRNDiagEntry(record))
            }
        }
        stateLock.lock()
        eventsTask = task
        otaEventsTask = otaTask
        self.diagTask = diagTask
        stateLock.unlock()

        await applyOtaPollConfig(Self.loadOtaPollConfig())

        if let restore = Self.registry.first(where: { $0.available && $0.hasCredentials() }) {
            try? await setActiveProvider(id: restore.id)
        }
    }

    public func stop() async {
        stateLock.lock()
        let auth = authTask
        let events = eventsTask
        let ota = otaEventsTask
        let diag = diagTask
        let companion = self.companion
        self.companion = nil
        eventsTask = nil
        otaEventsTask = nil
        authTask = nil
        diagTask = nil
        stateLock.unlock()

        auth?.cancel()
        events?.cancel()
        ota?.cancel()
        diag?.cancel()

        await companion?.stop()

        stateLock.lock()
        peers.removeAll()
        lastNowPlaying = nil
        stateLock.unlock()
        emitNowPlaying(nil)
    }

    // MARK: - Provider selection

    public func availableProviders() async -> [BridgethingProviderInfo] {
        Self.registry.map {
            BridgethingProviderInfo(id: $0.id, displayName: $0.displayName, available: $0.available)
        }
    }

    public func spotifyAuthConfig() async -> BridgethingSpotifyAuthConfig { BridgethingApp.spotifyAuthConfig() }

    public func completeSpotifySignIn(accessToken: String, refreshToken: String, usesDealer: Bool) async throws {
        BridgethingApp.persistSpotifyTokens(access: accessToken, refresh: refreshToken, usesDealer: usesDealer)
        try await setActiveProvider(id: BridgethingApp.spotifyProviderId)
    }

    public func setActiveProvider(id: String?) async throws {
        stateLock.lock(); let prevTask = authTask; stateLock.unlock()
        prevTask?.cancel()

        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            let task = Task { [weak self] in
                guard let self else {
                    continuation.resume(throwing: SessionError.deallocated)
                    return
                }
                do {
                    try await runSetActive(id: id)
                    continuation.resume()
                } catch is CancellationError {
                    emitAuth(.idleState())
                    continuation.resume(throwing: SessionError.cancelled)
                } catch {
                    emitAuth(.failed(message: String(describing: error)))
                    continuation.resume(throwing: error)
                }
            }
            stateLock.lock(); authTask = task; stateLock.unlock()
        }
    }

    public func cancelAuth() async {
        stateLock.lock()
        let task = authTask
        activeRegistration = nil
        stateLock.unlock()
        task?.cancel()
        let companion = stateLock.withLock { self.companion }
        try? await companion?.setActive(nil)
        emitProvider(nil)
        emitAuth(.idleState())
    }

    public func signOut() async {
        stateLock.lock()
        let task = authTask
        let registration = activeRegistration
        activeRegistration = nil
        stateLock.unlock()
        task?.cancel()

        // clear persisted credentials for the signed-in provider so it doesn't auto-restore.
        registration?.signOut()

        let companion = stateLock.withLock { self.companion }
        try? await companion?.setActive(nil)
        emitProvider(nil)
        emitServiceHealth(toRNServiceHealth(.ok))
        emitAuth(.idleState())
    }

    public func currentProvider() async -> BridgethingProviderInfo? {
        let companion = stateLock.withLock { self.companion }
        let glue = await companion?.current()
        return providerInfo(for: glue)
    }

    public func snapshot() async -> BridgethingSessionSnapshot {
        let companion = stateLock.withLock { self.companion }
        let glue = await companion?.current()
        let provider = providerInfo(for: glue)
        let ancs: BridgethingAncsAuthStatus =
            if let companion { toRNAncsAuthStatus(await companion.currentAncsAuthState()) } else { .unknown }

        var deviceMetaEntries: [BridgethingDeviceMetaEntry] = []
        if let companion {
            let ota = await companion.ota
            let peerIds = stateLock.withLock { Array(self.peers.keys) }
            for id in peerIds {
                if let meta = await ota.meta(deviceId: id) {
                    deviceMetaEntries.append(
                        BridgethingDeviceMetaEntry(deviceId: id, meta: Self.toRNDeviceMeta(meta))
                    )
                }
            }
        }

        let (peerList, nowPlaying, authState, serviceHealth) = stateLock.withLock {
            (Array(self.peers.values), self.lastNowPlaying, self.lastAuthState, self.lastServiceHealth)
        }

        return BridgethingSessionSnapshot(
            hostInfo: rnHostInfo(),
            provider: provider,
            authState: authState,
            serviceHealth: serviceHealth,
            peers: peerList,
            ancsAuthStatus: ancs,
            nowPlaying: nowPlaying,
            deviceMeta: deviceMetaEntries,
            capabilityFlags: Self.loadCapabilityFlags(),
            otaPollConfig: Self.loadOtaPollConfig()
        )
    }

    public func diagnosticsSnapshot(limit: Double) async -> [BridgethingDiagEntry] {
        DiagnosticsBuffer.shared.tail(limit: Int(limit)).map(Self.toRNDiagEntry)
    }

    public func companionDebug() async -> BridgethingCompanionDebug {
        let companion = stateLock.withLock { self.companion }
        let glue = await companion?.current()
        let debug = await glue?.debugState() ?? GlueDebugState()
        let ancs: BridgethingAncsAuthStatus =
            if let companion { toRNAncsAuthStatus(await companion.currentAncsAuthState()) } else { .unknown }
        return BridgethingCompanionDebug(
            authorityPlaybackHeld: debug.authorityPlaybackHeld,
            authorityMetadataHeld: debug.authorityMetadataHeld,
            baselinePollActive: debug.baselinePollActive,
            hintFetchActive: debug.hintFetchActive,
            ancsAuthStatus: ancs
        )
    }

    // MARK: - ANCS

    public func enableAncsNotifications() async -> BridgethingAncsSetupResult {
        let companion = stateLock.withLock { self.companion }
        guard let companion else {
            return BridgethingAncsSetupResult(
                kind: .failed,
                authStatus: .unknown,
                message: "session not started"
            )
        }
        let result = await companion.enableAncsNotifications()
        return toRNAncsSetupResult(result)
    }

    public func ancsAuthStatus() async -> BridgethingAncsAuthStatus {
        let companion = stateLock.withLock { self.companion }
        guard let companion else { return .unknown }
        return toRNAncsAuthStatus(await companion.currentAncsAuthState())
    }

    // MARK: - Webapps (per-device)

    public func listWebapps(deviceId: String) async throws -> [BridgethingWebappInfo] {
        let companion = try requirePeerConnected(deviceId)
        let result = try await companion.gateway.webapp.list(deviceId: deviceId)
        let value = try unwrapVoidErr(result, label: "listWebapps")
        return value.webapps
            .filter { $0.role != .launcher }
            .map(Self.toRNWebappInfo)
    }

    public func currentWebapp(deviceId: String) async throws -> BridgethingActiveWebapp? {
        let companion = try requirePeerConnected(deviceId)
        let result = try await companion.gateway.webapp.getActive(deviceId: deviceId)
        let value = try unwrapVoidErr(result, label: "currentWebapp")
        guard let id = value.id else { return nil }
        return BridgethingActiveWebapp(id: id.uuidString.lowercased(), name: value.name)
    }

    public func installWebapp(deviceId: String, sourceUri: String) async throws -> BridgethingWebappInfo {
        let companion = try requirePeerConnected(deviceId)
        let (archiveUrl, isTemporary) = try await Self.resolveArchive(sourceUri)
        defer { if isTemporary { try? FileManager.default.removeItem(at: archiveUrl) } }

        let size = try Self.fileSize(archiveUrl)
        guard size <= Int(UInt32.max) else {
            throw SessionError.invalidArchive
        }
        let installId = try Self.sha256HexOfFile(archiveUrl)
        let begin = WebappInstallBegin(
            installId: installId,
            expectedSha256: installId,
            expectedSize: UInt32(size)
        )
        let beginResult = try await companion.gateway.webapp.installBegin(deviceId: deviceId, begin)
        let ack = try unwrapWebappErr(beginResult, label: "installBegin")

        // subscribe before the last chunk to avoid racing the daemon's installed broadcast.
        let installedTask = Task<WebappInfo, Error> {
            for await pair in companion.gateway.webapp.webappInstalled where pair.deviceId == deviceId {
                return pair.msg
            }
            throw SessionError.installInterrupted
        }
        let failedTask = Task<WebappError, Error> {
            for await pair in companion.gateway.webapp.webappInstallFailed
                where pair.deviceId == deviceId && pair.msg.installId == installId {
                return pair.msg.error
            }
            throw SessionError.installInterrupted
        }

        do {
            try await streamInstallChunks(
                gateway: companion.gateway,
                deviceId: deviceId,
                installId: installId,
                archiveUrl: archiveUrl,
                total: size,
                startOffset: ack.resumeFromOffset
            )
        } catch {
            installedTask.cancel()
            failedTask.cancel()
            throw error
        }

        let info: WebappInfo
        do {
            info = try await withThrowingTaskGroup(of: InstallOutcome.self) { group in
                group.addTask { .installed(try await installedTask.value) }
                group.addTask { .failed(try await failedTask.value) }
                group.addTask {
                    try await Task.sleep(nanoseconds: 60_000_000_000)
                    throw SessionError.installTimedOut
                }
                defer { group.cancelAll() }
                guard let first = try await group.next() else {
                    throw SessionError.installInterrupted
                }
                switch first {
                case let .installed(value):
                    return value
                case let .failed(err):
                    throw SessionError.webappError(err)
                }
            }
        } catch {
            installedTask.cancel()
            failedTask.cancel()
            throw error
        }

        emitWebappsChanged(deviceId)
        return Self.toRNWebappInfo(info)
    }

    private func streamInstallChunks(
        gateway: BridgethingGateway,
        deviceId: String,
        installId: String,
        archiveUrl: URL,
        total: Int,
        startOffset: UInt32
    ) async throws {
        let chunkSize = 64 * 1024
        let handle = try FileHandle(forReadingFrom: archiveUrl)
        defer { try? handle.close() }
        var offset = Int(startOffset)
        try handle.seek(toOffset: UInt64(offset))
        while offset < total {
            let want = min(chunkSize, total - offset)
            let slice = try handle.read(upToCount: want) ?? Data()
            guard !slice.isEmpty else { throw SessionError.invalidArchive }
            let end = offset + slice.count
            let chunk = WebappInstallChunk(
                installId: installId,
                offset: UInt32(offset),
                bytes: slice,
                last: end == total
            )
            try await gateway.device(deviceId).webapp.installChunk(chunk, priority: .bulk)
            offset = end
        }
    }

    private static func resolveArchive(_ sourceUri: String) async throws -> (URL, Bool) {
        guard let url = URL(string: sourceUri) else { throw SessionError.invalidArchive }
        if url.isFileURL { return (url, false) }
        guard let scheme = url.scheme?.lowercased(), scheme == "http" || scheme == "https" else {
            throw SessionError.invalidArchive
        }
        let (tempUrl, response) = try await URLSession.shared.download(from: url)
        if let http = response as? HTTPURLResponse, !(200...299).contains(http.statusCode) {
            try? FileManager.default.removeItem(at: tempUrl)
            throw SessionError.downloadFailed("download failed: \(http.statusCode)")
        }
        return (tempUrl, true)
    }

    private static func fileSize(_ url: URL) throws -> Int {
        let values = try url.resourceValues(forKeys: [.fileSizeKey])
        guard let size = values.fileSize else { throw SessionError.invalidArchive }
        return size
    }

    private static func sha256HexOfFile(_ url: URL) throws -> String {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        var hasher = SHA256()
        while let block = try handle.read(upToCount: 1024 * 1024), !block.isEmpty {
            hasher.update(data: block)
        }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }

    public func uninstallWebapp(deviceId: String, id: String) async throws {
        let uuid = try parseUuid(id)
        let companion = try requirePeerConnected(deviceId)
        let req = WebappUninstall(id: uuid)
        let result = try await companion.gateway.webapp.uninstall(deviceId: deviceId, req)
        _ = try unwrapWebappErr(result, label: "uninstallWebapp")
        emitWebappsChanged(deviceId)
    }

    public func switchWebapp(deviceId: String, id: String) async throws {
        let uuid = try parseUuid(id)
        let companion = try requirePeerConnected(deviceId)
        let req = WebappSwitchTo(id: uuid)
        let result = try await companion.gateway.webapp.switchTo(deviceId: deviceId, req)
        _ = try unwrapWebappErr(result, label: "switchWebapp")
        emitWebappsChanged(deviceId)
    }

    public func webappIcon(deviceId: String, id: String) async throws -> BridgethingWebappIcon? {
        let uuid = try parseUuid(id)
        let companion = try requirePeerConnected(deviceId)
        let req = WebappIcon(id: uuid)
        let result = try await companion.gateway.webapp.icon(deviceId: deviceId, req)
        switch result {
        case let .ok(reply):
            if reply.mime == "image/svg+xml", let svg = String(data: reply.bytes, encoding: .utf8) {
                return BridgethingWebappIcon(fileUri: nil, svg: svg, mime: reply.mime)
            }
            let url = try Self.writeIconToCache(deviceId: deviceId, id: id, mime: reply.mime, bytes: reply.bytes)
            return BridgethingWebappIcon(fileUri: url.absoluteString, svg: nil, mime: reply.mime)
        case let .domain(err):
            if case .iconNotAvailable = err { return nil }
            throw SessionError.webappError(err)
        case let .protocolError(err):
            throw SessionError.protocolError(err)
        }
    }

    public func listWebappConfig(deviceId: String, id: String) async throws -> [BridgethingConfigEntry] {
        let uuid = try parseUuid(id)
        let companion = try requirePeerConnected(deviceId)
        let req = WebappConfigList(id: uuid)
        let result = try await companion.gateway.webapp.configList(deviceId: deviceId, req)
        let reply = try unwrapWebappErr(result, label: "listWebappConfig")
        return reply.entries.map { BridgethingConfigEntry(key: $0.key, value: $0.value) }
    }

    public func setWebappConfigField(deviceId: String, id: String, key: String, value: String) async throws {
        let uuid = try parseUuid(id)
        let companion = try requirePeerConnected(deviceId)
        let req = WebappConfigSet(id: uuid, key: key, value: value)
        let result = try await companion.gateway.webapp.configSet(deviceId: deviceId, req)
        _ = try unwrapWebappErr(result, label: "setWebappConfigField")
    }

    public func deleteWebappConfigField(deviceId: String, id: String, key: String) async throws {
        let uuid = try parseUuid(id)
        let companion = try requirePeerConnected(deviceId)
        let req = WebappConfigDelete(id: uuid, key: key)
        let result = try await companion.gateway.webapp.configDelete(deviceId: deviceId, req)
        _ = try unwrapWebappErr(result, label: "deleteWebappConfigField")
    }

    // MARK: - Capability flags

    public func setCapabilityFlags(flags: BridgethingCapabilityFlags) async {
        Self.saveCapabilityFlags(flags)
        let companion = stateLock.withLock { self.companion }
        await companion?.setCapabilityFlags(Self.toCompanionFlags(flags))
    }

    // MARK: - OTA

    public func setOtaPollConfig(config: BridgethingOtaPollConfig?) async {
        Self.saveOtaPollConfig(config)
        await applyOtaPollConfig(config)
    }

    private func applyOtaPollConfig(_ config: BridgethingOtaPollConfig?) async {
        let companion = stateLock.withLock { self.companion }
        let ota = await companion?.ota
        if let config {
            let mapped = OtaPollConfig(
                rootURL: config.rootUrl.flatMap(URL.init(string:)) ?? URL(string: "https://ota.bridgething.com")!,
                channel: config.channel,
                intervalSeconds: max(60, config.intervalSeconds),
                cacheDirectory: nil,
                autoPush: config.autoPush
            )
            await ota?.setPollConfig(mapped)
        } else {
            await ota?.setPollConfig(nil)
        }
    }

    public func checkForOtaUpdate(channel: String, rootUrl: String?) async {
        let companion = stateLock.withLock { self.companion }
        let ota = await companion?.ota
        await ota?.checkNow(channel: channel, rootURL: Self.otaRootURL(rootUrl))
    }

    public func fetchOtaManifest(rootUrl: String?) async throws -> BridgethingOtaManifest {
        let companion = stateLock.withLock { self.companion }
        guard let ota = await companion?.ota else { throw SessionError.cancelled }
        let manifest = try await ota.discoverManifest(rootURL: Self.otaRootURL(rootUrl))
        return Self.toRNOtaManifest(manifest)
    }

    public func applyOtaUpdate(deviceId: String, channel: String, version: String, rootUrl: String?) async throws {
        let companion = stateLock.withLock { self.companion }
        let ota = await companion?.ota
        await ota?.applyVersion(deviceId: deviceId, channel: channel, version: version, rootURL: Self.otaRootURL(rootUrl))
    }

    public func reconnectPeer(deviceId: String) async throws {
        let companion = stateLock.withLock { self.companion }
        guard let companion else { return }
        try await companion.gateway.reconnect(deviceId: deviceId)
    }

    private static func otaRootURL(_ raw: String?) -> URL {
        raw.flatMap(URL.init(string:)) ?? URL(string: "https://ota.bridgething.com")!
    }

    // MARK: - Host identity

    private func rnHostInfo() -> BridgethingHostInfo {
        let host = Self.makeHostInfo()
        return BridgethingHostInfo(
            appName: host.appName,
            appVersion: host.appVersion,
            osName: host.osName,
            osVersion: host.osVersion,
            hostIdentifier: host.address,
            libVersion: BridgethingCompanionVersion.lib,
            libbridgethingVersion: BridgethingCompanionVersion.libbridgething,
            adapterVersion: host.adapterVersion
        )
    }

    private static func makeHostInfo() -> HostInfo {
        let base = Self.hostInfo
        let identifier = UIDevice.current.identifierForVendor?.uuidString ?? ""
        return HostInfo(
            appName: base.appName,
            appVersion: base.appVersion,
            osName: base.osName,
            osVersion: UIDevice.current.systemVersion,
            address: identifier,
            adapterVersion: "eaccessory"
        )
    }

    // MARK: - Native-authoritative persistence (caps + OTA poll config)

    private static let defaults = UserDefaults.standard

    private enum PrefKey {
        static let capsConfigured = "bridgething.caps.configured"
        static let capsGeo = "bridgething.caps.geo"
        static let capsNotifications = "bridgething.caps.notifications"
        static let capsNetFetch = "bridgething.caps.netFetch"
        static let capsNetWs = "bridgething.caps.netWs"
        static let capsAudioTts = "bridgething.caps.audioTts"
        static let otaConfigured = "bridgething.ota.configured"
        static let otaChannel = "bridgething.ota.channel"
        static let otaInterval = "bridgething.ota.intervalSeconds"
        static let otaAutoPush = "bridgething.ota.autoPush"
        static let otaRootUrl = "bridgething.ota.rootUrl"
    }

    private static func loadCapabilityFlags() -> BridgethingCapabilityFlags {
        guard defaults.bool(forKey: PrefKey.capsConfigured) else {
            return BridgethingCapabilityFlags(
                geo: true, notifications: true, netFetch: true, netWs: true, audioTts: true
            )
        }
        return BridgethingCapabilityFlags(
            geo: defaults.bool(forKey: PrefKey.capsGeo),
            notifications: defaults.bool(forKey: PrefKey.capsNotifications),
            netFetch: defaults.bool(forKey: PrefKey.capsNetFetch),
            netWs: defaults.bool(forKey: PrefKey.capsNetWs),
            audioTts: defaults.bool(forKey: PrefKey.capsAudioTts)
        )
    }

    private static func saveCapabilityFlags(_ f: BridgethingCapabilityFlags) {
        defaults.set(true, forKey: PrefKey.capsConfigured)
        defaults.set(f.geo, forKey: PrefKey.capsGeo)
        defaults.set(f.notifications, forKey: PrefKey.capsNotifications)
        defaults.set(f.netFetch, forKey: PrefKey.capsNetFetch)
        defaults.set(f.netWs, forKey: PrefKey.capsNetWs)
        defaults.set(f.audioTts, forKey: PrefKey.capsAudioTts)
    }

    private static func loadCompanionCapabilityFlags() -> CompanionCapabilityFlags {
        toCompanionFlags(loadCapabilityFlags())
    }

    private static func toCompanionFlags(_ f: BridgethingCapabilityFlags) -> CompanionCapabilityFlags {
        CompanionCapabilityFlags(
            geo: f.geo, notifications: f.notifications, netFetch: f.netFetch, netWs: f.netWs, audioTts: f.audioTts
        )
    }

    private static func loadOtaPollConfig() -> BridgethingOtaPollConfig? {
        guard defaults.bool(forKey: PrefKey.otaConfigured) else { return nil }
        let root = defaults.string(forKey: PrefKey.otaRootUrl)
        return BridgethingOtaPollConfig(
            channel: defaults.string(forKey: PrefKey.otaChannel) ?? "stable",
            intervalSeconds: defaults.double(forKey: PrefKey.otaInterval),
            autoPush: defaults.bool(forKey: PrefKey.otaAutoPush),
            rootUrl: (root?.isEmpty == false) ? root : nil
        )
    }

    private static func saveOtaPollConfig(_ config: BridgethingOtaPollConfig?) {
        guard let config else {
            defaults.set(false, forKey: PrefKey.otaConfigured)
            return
        }
        defaults.set(true, forKey: PrefKey.otaConfigured)
        defaults.set(config.channel, forKey: PrefKey.otaChannel)
        defaults.set(config.intervalSeconds, forKey: PrefKey.otaInterval)
        defaults.set(config.autoPush, forKey: PrefKey.otaAutoPush)
        defaults.set(config.rootUrl, forKey: PrefKey.otaRootUrl)
    }

    // MARK: - Diagnostics record conversion

    private static func toRNDiagEntry(_ r: DiagRecord) -> BridgethingDiagEntry {
        BridgethingDiagEntry(
            seq: Double(r.seq),
            ts: r.timestampMs,
            kind: rnDiagKind(r.kind),
            deviceId: r.deviceId,
            direction: r.direction.map(rnDiagDirection),
            frameKind: r.frameKind.map(rnDiagFrameKind),
            surface: r.surface,
            byteSize: r.byteSize.map(Double.init),
            requestId: r.requestId,
            latencyMs: r.latencyMs,
            level: r.level,
            target: r.target,
            message: r.message,
            category: r.category,
            detail: r.detail,
            fields: r.fields.map { $0.map { BridgethingConfigEntry(key: $0.key, value: $0.value) } }
        )
    }

    private static func rnDiagKind(_ k: DiagRecord.Kind) -> BridgethingDiagKind {
        switch k {
        case .frame: .frame
        case .log: .log
        case .breadcrumb: .breadcrumb
        }
    }

    private static func rnDiagDirection(_ d: DiagRecord.Direction) -> BridgethingDiagDirection {
        switch d {
        case .outbound: .outbound
        case .inbound: .inbound
        }
    }

    private static func rnDiagFrameKind(_ f: DiagRecord.FrameKind) -> BridgethingDiagFrameKind {
        switch f {
        case .request: .request
        case .response: .response
        case .event: .event
        case .command: .command
        }
    }

    // MARK: - Callback setters

    public func setOnProviderChanged(_ callback: @escaping @Sendable (BridgethingProviderInfo?) -> Void) {
        stateLock.withLock { onProviderChanged = callback }
    }

    public func setOnAuthStateChanged(_ callback: @escaping @Sendable (BridgethingAuthState) -> Void) {
        stateLock.withLock { onAuthStateChanged = callback }
    }

    public func setOnServiceHealthChanged(_ callback: @escaping @Sendable (BridgethingServiceHealth) -> Void) {
        stateLock.withLock { onServiceHealthChanged = callback }
    }

    public func setOnPeerConnected(_ callback: @escaping @Sendable (BridgethingSessionPeer) -> Void) {
        stateLock.withLock { onPeerConnected = callback }
    }

    public func setOnPeerDisconnected(_ callback: @escaping @Sendable (String) -> Void) {
        stateLock.withLock { onPeerDisconnected = callback }
    }

    public func setOnPeerLinkFailed(_ callback: @escaping @Sendable (BridgethingSessionPeer) -> Void) {
        stateLock.withLock { onPeerLinkFailed = callback }
    }

    public func setOnNowPlayingChanged(_ callback: @escaping @Sendable (BridgethingNowPlaying?) -> Void) {
        stateLock.withLock { onNowPlayingChanged = callback }
    }

    public func setOnAncsAuthStatusChanged(_ callback: @escaping @Sendable (BridgethingAncsAuthStatus) -> Void) {
        stateLock.withLock { onAncsAuthStatusChanged = callback }
    }

    public func setOnLog(_ callback: @escaping @Sendable (String, String) -> Void) {
        stateLock.withLock { onLog = callback }
    }

    public func setLogStreamingEnabled(_ enabled: Bool) {
        let companion: BridgethingCompanion? = stateLock.withLock {
            logStreamingDesired = enabled
            return self.companion
        }
        guard let companion else { return }
        if enabled {
            Task { [weak self] in
                await companion.setLogObserver { [weak self] level, message in
                    self?.emitLog(level.rawValue, message)
                }
                await companion.setDeviceLogStreaming(true)
            }
        } else {
            Task {
                await companion.setDeviceLogStreaming(false)
                await companion.setLogObserver(nil)
            }
        }
    }

    public func setOnWebappsChanged(_ callback: @escaping @Sendable (String) -> Void) {
        stateLock.withLock { onWebappsChanged = callback }
    }

    public func setOnDeviceMetaChanged(_ callback: @escaping @Sendable (String, BridgethingDeviceMeta) -> Void) {
        stateLock.withLock { onDeviceMetaChanged = callback }
    }

    public func setOnOtaEvent(_ callback: @escaping @Sendable (BridgethingOtaEvent) -> Void) {
        stateLock.withLock { onOtaEvent = callback }
    }

    public func setOnDiagEntry(_ callback: @escaping @Sendable (BridgethingDiagEntry) -> Void) {
        stateLock.withLock { onDiagEntry = callback }
    }

    // MARK: - Cross-platform AccessorySetupKit picker

    public func presentPairPicker() async throws -> BridgethingBtDevice? {
        let companion = try requireCompanion()
        guard let result = await companion.presentPairPicker() else { return nil }
        return BridgethingBtDevice(
            address: result.id,
            name: result.name,
            bondState: .bonded,
            isCarThing: true
        )
    }

    // MARK: - Android-only surfaces (iOS stubs)

    public func isNotificationAccessGranted() async -> Bool { false }
    public func requestNotificationAccess() async throws { throw SessionError.unsupportedOnPlatform }
    public func isDefaultDialer() async -> Bool { false }
    public func requestDefaultDialer() async throws { throw SessionError.unsupportedOnPlatform }
    public func revokeRuntimePermissions(permissions: [String]) async -> Bool { false }
    public func killApp() async {
        // apple rejects explicit process termination.
    }

    // MARK: - Internal

    private func runSetActive(id: String?) async throws {
        let companion = stateLock.withLock { self.companion }
        guard let companion else { throw SessionError.notStarted }

        if let id {
            guard let registration = Self.registry.first(where: { $0.id == id }) else {
                throw SessionError.unknownProvider(id)
            }

            let glue = registration.factory()
            stateLock.withLock { activeRegistration = registration }
            // subscribe before setActive; glue may emit authenticated synchronously during attach.
            await glue.setAuthObserver { [weak self] state in
                self?.handleGlueAuthState(state)
            }
            await glue.setServiceHealthObserver { [weak self] health in
                self?.emitServiceHealth(toRNServiceHealth(health))
            }
            try await companion.setActive(glue)
            try Task.checkCancellation()
        } else {
            stateLock.withLock { activeRegistration = nil }
            try await companion.setActive(nil)
            emitProvider(nil)
            emitAuth(.idleState())
        }
    }

    private func handleGlueAuthState(_ state: GlueAuthState) {
        switch state {
        case let .pending(prompt):
            emitAuth(.pendingState(
                userCode: prompt?.userCode,
                verificationUrl: prompt?.verificationURL.absoluteString,
                verificationUrlComplete: prompt?.verificationURLComplete.absoluteString
            ))
        case .authenticated:
            if let registration = stateLock.withLock({ activeRegistration }) {
                emitProvider(BridgethingProviderInfo(
                    id: registration.id,
                    displayName: registration.displayName,
                    available: registration.available
                ))
            }
            emitAuth(.authenticated())
        case let .failed(message):
            emitAuth(.failed(message: message))
        }
    }

    private func handleGatewayEvent(_ event: GatewayEvent) {
        switch event {
        case let .connected(device):
            let peer = BridgethingSessionPeer(id: device.id, name: device.name, status: .connected, linkError: nil)
            stateLock.withLock { peers[device.id] = peer }
            emitPeerConnected(peer)
        case let .disconnected(id):
            stateLock.withLock { _ = peers.removeValue(forKey: id) }
            emitPeerDisconnected(id)
        case let .linkFailed(device, reason):
            let peer = BridgethingSessionPeer(id: device.id, name: device.name, status: .linkfailed, linkError: reason)
            stateLock.withLock { peers[device.id] = peer }
            emitPeerLinkFailed(peer)
        case let .message(deviceId, msg):
            if case let .version(meta) = msg.data {
                emitDeviceMetaChanged(deviceId, Self.toRNDeviceMeta(meta))
            }
        case let .decodeError(id, description):
            emitLog("warn", "[\(id)] decode error: \(description)")
        }
    }

    private func handleNowPlaying(_ glue: GlueNowPlaying?) {
        let rn: BridgethingNowPlaying? = glue.flatMap(Self.toRNNowPlaying)
        stateLock.withLock { lastNowPlaying = rn }
        emitNowPlaying(rn)
    }

    private func providerInfo(for glue: (any BridgethingGlue)?) -> BridgethingProviderInfo? {
        guard let glue else { return nil }
        let registration = Self.registry.first { $0.id == type(of: glue).name }
        return BridgethingProviderInfo(
            id: type(of: glue).name,
            displayName: registration?.displayName ?? type(of: glue).displayName,
            available: registration?.available ?? true
        )
    }

    private func requirePeerConnected(_ deviceId: String) throws -> BridgethingCompanion {
        let (companion, connected) = stateLock.withLock { (self.companion, peers[deviceId] != nil) }
        guard let companion else { throw SessionError.notStarted }
        guard connected else { throw SessionError.noPeerConnected(deviceId) }
        return companion
    }

    private func requireCompanion() throws -> BridgethingCompanion {
        guard let companion = stateLock.withLock({ self.companion }) else {
            throw SessionError.notStarted
        }
        return companion
    }

    private func parseUuid(_ id: String) throws -> UUID {
        guard let uuid = UUID(uuidString: id) else {
            throw SessionError.invalidUuid(id)
        }
        return uuid
    }

    private func unwrapVoidErr<T>(_ result: RequestResult<T, Never>, label: String) throws -> T {
        switch result {
        case let .ok(value):
            return value
        case .domain:
            // Never is uninhabited; unreachable.
            fatalError("RequestResult<_, Never>.domain is uninhabited")
        case let .protocolError(err):
            throw SessionError.protocolError(err)
        }
    }

    private func unwrapWebappErr<T>(_ result: RequestResult<T, WebappError>, label: String) throws -> T {
        switch result {
        case let .ok(value):
            return value
        case let .domain(err):
            throw SessionError.webappError(err)
        case let .protocolError(err):
            throw SessionError.protocolError(err)
        }
    }

    /// same (deviceId, id) always writes the same path so the RN image cache stays valid.
    private static func writeIconToCache(deviceId: String, id: String, mime: String?, bytes: Data) throws -> URL {
        let caches = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first!
        let dir = caches.appendingPathComponent("bridgething-webapp-icons", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let ext: String = {
            switch mime {
            case "image/png": return "png"
            case "image/jpeg", "image/jpg": return "jpg"
            case "image/webp": return "webp"
            case "image/svg+xml": return "svg"
            default: return "bin"
            }
        }()
        let safeDevice = deviceId.replacingOccurrences(of: "/", with: "_")
        let safeId = id.replacingOccurrences(of: "/", with: "_")
        let url = dir.appendingPathComponent("\(safeDevice)__\(safeId).\(ext)")
        try bytes.write(to: url, options: .atomic)
        return url
    }

    // MARK: - Emit helpers

    private func emitProvider(_ info: BridgethingProviderInfo?) {
        stateLock.withLock { onProviderChanged }?(info)
    }

    private func emitServiceHealth(_ health: BridgethingServiceHealth) {
        let cb = stateLock.withLock { () -> (@Sendable (BridgethingServiceHealth) -> Void)? in
            lastServiceHealth = health
            return onServiceHealthChanged
        }
        cb?(health)
    }

    private func emitAuth(_ state: BridgethingAuthState) {
        let cb = stateLock.withLock { () -> (@Sendable (BridgethingAuthState) -> Void)? in
            lastAuthState = state
            return onAuthStateChanged
        }
        cb?(state)
    }

    private func emitPeerConnected(_ peer: BridgethingSessionPeer) {
        stateLock.withLock { onPeerConnected }?(peer)
    }

    private func emitPeerDisconnected(_ id: String) {
        stateLock.withLock { onPeerDisconnected }?(id)
    }

    private func emitPeerLinkFailed(_ peer: BridgethingSessionPeer) {
        stateLock.withLock { onPeerLinkFailed }?(peer)
    }

    private func emitNowPlaying(_ np: BridgethingNowPlaying?) {
        stateLock.withLock { onNowPlayingChanged }?(np)
    }

    private func emitAncsAuthStatus(_ status: BridgethingAncsAuthStatus) {
        stateLock.withLock { onAncsAuthStatusChanged }?(status)
    }

    private func emitLog(_ level: String, _ message: String) {
        stateLock.withLock { onLog }?(level, message)
    }

    private func emitWebappsChanged(_ deviceId: String) {
        stateLock.withLock { onWebappsChanged }?(deviceId)
    }

    private func emitDeviceMetaChanged(_ deviceId: String, _ meta: BridgethingDeviceMeta) {
        stateLock.withLock { onDeviceMetaChanged }?(deviceId, meta)
    }

    private func emitOtaEvent(_ event: BridgethingOtaEvent) {
        stateLock.withLock { onOtaEvent }?(event)
    }

    // MARK: - Wire → RN conversion

    private static func toRNNowPlaying(_ glue: GlueNowPlaying) -> BridgethingNowPlaying {
        let item = glue.update.mediaItem
        let track: BridgethingNowPlayingTrack? = item.map { mi in
            BridgethingNowPlayingTrack(
                id: mi.persistentId,
                title: mi.title,
                artist: mi.artist,
                album: mi.album,
                artworkUrl: glue.artworkUrl,
                durationMs: mi.durationMs.map { Double($0) }
            )
        }
        let pb = glue.update.playback
        let mode: BridgethingRepeatMode = switch pb?.`repeat` ?? .off {
        case .off: .off
        case .one: .one
        case .all: .all
        }
        let playback = BridgethingNowPlayingPlayback(
            playing: pb?.playing ?? false,
            positionMs: Double(pb?.positionMs ?? 0),
            shuffle: pb?.shuffle ?? false,
            repeatMode: mode
        )
        return BridgethingNowPlaying(track: track, playback: playback, appName: pb?.appDisplayName)
    }

    private static func toRNDeviceMeta(_ meta: BridgeThingMeta) -> BridgethingDeviceMeta {
        BridgethingDeviceMeta(
            daemonVersion: meta.appVersion,
            imageVersion: meta.imageVersion,
            appName: meta.appName,
            osName: meta.osName,
            osVersion: meta.osVersion,
            channel: meta.channel,
            modelName: meta.modelName,
            serialNumber: meta.serialNumber
        )
    }

    private static func toRNOtaManifest(_ m: OtaDiscoverManifest) -> BridgethingOtaManifest {
        let channels = m.channels.map { (slug, ch) -> BridgethingOtaChannelInfo in
            let releases = ch.releases.compactMap { v -> BridgethingOtaRelease? in
                guard let composite = OtaCompositeVersion.parse(v) else { return nil }
                let rel = m.releases[v]
                return BridgethingOtaRelease(
                    version: v,
                    daemonVersion: composite.daemon,
                    imageVersion: composite.image,
                    yanked: rel?.yanked != nil,
                    deprecated: rel?.deprecated ?? false
                )
            }
            return BridgethingOtaChannelInfo(
                slug: slug,
                name: ch.name,
                stability: ch.stability,
                isDefault: ch.isDefault,
                latest: ch.latest,
                releases: releases
            )
        }
        return BridgethingOtaManifest(updatedAt: m.updatedAt, channels: channels)
    }

    private static func toRNWebappInfo(_ info: BridgethingSchema.WebappInfo) -> BridgethingWebappInfo {
        BridgethingWebappInfo(
            id: info.id.uuidString.lowercased(),
            name: info.name,
            source: info.source == .builtin ? .builtin : .installed,
            role: info.role == .launcher ? .launcher : .standard,
            version: info.version,
            description: info.description,
            iconAvailable: info.iconAvailable,
            iconMime: info.iconMime,
            config: info.config.map(toRNConfigField),
            permissions: info.permissions
        )
    }

    private static func toRNConfigField(_ field: ConfigField) -> BridgethingConfigField {
        switch field {
        case let .string(f):
            return BridgethingConfigField(
                kind: .string,
                key: f.key,
                label: f.label,
                pattern: f.pattern,
                minLength: f.minLength.map(Double.init),
                maxLength: f.maxLength.map(Double.init),
                min: nil,
                max: nil,
                step: nil,
                choices: nil,
                defaultValue: f.default
            )
        case let .secret(f):
            return BridgethingConfigField(
                kind: .secret,
                key: f.key,
                label: f.label,
                pattern: f.pattern,
                minLength: f.minLength.map(Double.init),
                maxLength: f.maxLength.map(Double.init),
                min: nil,
                max: nil,
                step: nil,
                choices: nil,
                defaultValue: f.default
            )
        case let .number(f):
            return BridgethingConfigField(
                kind: .number,
                key: f.key,
                label: f.label,
                pattern: nil,
                minLength: nil,
                maxLength: nil,
                min: f.min,
                max: f.max,
                step: f.step,
                choices: nil,
                defaultValue: f.default.map { "\($0)" }
            )
        case let .boolean(f):
            return BridgethingConfigField(
                kind: .boolean,
                key: f.key,
                label: f.label,
                pattern: nil,
                minLength: nil,
                maxLength: nil,
                min: nil,
                max: nil,
                step: nil,
                choices: nil,
                defaultValue: f.default.map { $0 ? "true" : "false" }
            )
        case let .enum(f):
            return BridgethingConfigField(
                kind: .enum,
                key: f.key,
                label: f.label,
                pattern: nil,
                minLength: nil,
                maxLength: nil,
                min: nil,
                max: nil,
                step: nil,
                choices: f.choices,
                defaultValue: f.default
            )
        }
    }
}

private enum SessionError: Error {
    case deallocated
    case cancelled
    case notStarted
    case unknownProvider(String)
    case noPeerConnected(String)
    case invalidUuid(String)
    case invalidArchive
    case downloadFailed(String)
    case installInterrupted
    case installTimedOut
    case webappError(WebappError)
    case protocolError(WireError)
    case unsupportedOnPlatform
}

private enum InstallOutcome {
    case installed(WebappInfo)
    case failed(WebappError)
}

private extension BridgethingAuthState {
    static func idleState() -> BridgethingAuthState {
        .init(kind: .idle, userCode: nil, verificationUrl: nil, verificationUrlComplete: nil, message: nil)
    }
    static func pendingState(userCode: String?, verificationUrl: String?, verificationUrlComplete: String?) -> BridgethingAuthState {
        .init(kind: .pending, userCode: userCode, verificationUrl: verificationUrl, verificationUrlComplete: verificationUrlComplete, message: nil)
    }
    static func authenticated() -> BridgethingAuthState {
        .init(kind: .authenticated, userCode: nil, verificationUrl: nil, verificationUrlComplete: nil, message: nil)
    }
    static func failed(message: String) -> BridgethingAuthState {
        .init(kind: .failed, userCode: nil, verificationUrl: nil, verificationUrlComplete: nil, message: message)
    }
}

private extension NSLock {
    @discardableResult
    func withLock<T>(_ body: () throws -> T) rethrows -> T {
        lock(); defer { unlock() }
        return try body()
    }
}

private func toRNAncsAuthStatus(_ state: AncsAuthState) -> BridgethingAncsAuthStatus {
    switch state {
    case .unknown: .unknown
    case .probing: .probing
    case .authorized: .authorized
    case .unauthorized: .unauthorized
    }
}

private func toRNServiceHealth(_ health: GlueServiceHealth) -> BridgethingServiceHealth {
    switch health {
    case .ok:
        BridgethingServiceHealth(kind: .ok, retryAfterSeconds: nil)
    case let .rateLimited(retryAfterSeconds):
        BridgethingServiceHealth(kind: .ratelimited, retryAfterSeconds: Double(retryAfterSeconds))
    case .unreachable:
        BridgethingServiceHealth(kind: .unreachable, retryAfterSeconds: nil)
    }
}

private func toRNAncsSetupResult(_ result: AncsSetupResult) -> BridgethingAncsSetupResult {
    let (kind, message): (BridgethingAncsSetupKind, String?) = switch result.kind {
    case .paired: (.paired, nil)
    case .alreadyPaired: (.alreadypaired, nil)
    case .cancelled: (.cancelled, nil)
    case .unsupported: (.unsupported, nil)
    case let .failed(reason): (.failed, reason)
    }
    return BridgethingAncsSetupResult(
        kind: kind,
        authStatus: toRNAncsAuthStatus(result.authState),
        message: message
    )
}

private func toRNOtaEvent(_ event: OtaPollEvent) -> BridgethingOtaEvent {
    switch event {
    case let .manifestPolled(updatedAt):
        return BridgethingOtaEvent(
            kind: .manifestpolled,
            updatedAt: updatedAt,
            reason: nil, deviceId: nil, otaKind: nil,
            fromVersion: nil, toVersion: nil, phase: nil, percent: nil,
            deviceChannel: nil, configuredChannel: nil
        )
    case let .manifestPollFailed(reason):
        return BridgethingOtaEvent(
            kind: .manifestpollfailed,
            updatedAt: nil, reason: reason, deviceId: nil, otaKind: nil,
            fromVersion: nil, toVersion: nil, phase: nil, percent: nil,
            deviceChannel: nil, configuredChannel: nil
        )
    case let .channelMismatch(deviceId, deviceChannel, configuredChannel):
        return BridgethingOtaEvent(
            kind: .channelmismatch,
            updatedAt: nil,
            reason: "device on \(deviceChannel), companion configured for \(configuredChannel)",
            deviceId: deviceId, otaKind: nil,
            fromVersion: nil, toVersion: nil, phase: nil, percent: nil,
            deviceChannel: deviceChannel, configuredChannel: configuredChannel
        )
    case let .updateAvailable(deviceId, kind, fromVersion, toVersion):
        return BridgethingOtaEvent(
            kind: .updateavailable,
            updatedAt: nil, reason: nil, deviceId: deviceId,
            otaKind: kind == .image ? .image : .daemon,
            fromVersion: fromVersion, toVersion: toVersion,
            phase: nil, percent: nil,
            deviceChannel: nil, configuredChannel: nil
        )
    case let .progress(deviceId, kind, snapshot):
        let (phase, percent, reason): (BridgethingOtaPhase, Double, String?) = {
            switch snapshot {
            case .idle: return (.idle, 0, nil)
            case let .streaming(p): return (.streaming, Double(p), nil)
            case let .applying(phase: ph, percent: p):
                let mapped: BridgethingOtaPhase = switch ph {
                case .streaming: .streaming
                case .verifying: .verifying
                case .writing: .writing
                case .confirming: .confirming
                case .reboot: .reboot
                }
                return (mapped, Double(p), nil)
            case .completed: return (.completed, 100, nil)
            case let .failed(r): return (.failed, 0, r)
            }
        }()
        return BridgethingOtaEvent(
            kind: .progress,
            updatedAt: nil, reason: reason, deviceId: deviceId,
            otaKind: kind == .image ? .image : .daemon,
            fromVersion: nil, toVersion: nil, phase: phase, percent: percent,
            deviceChannel: nil, configuredChannel: nil
        )
    case let .updated(deviceId, kind, version):
        return BridgethingOtaEvent(
            kind: .updated,
            updatedAt: nil, reason: nil, deviceId: deviceId,
            otaKind: kind == .image ? .image : .daemon,
            fromVersion: nil, toVersion: version, phase: nil, percent: nil,
            deviceChannel: nil, configuredChannel: nil
        )
    case let .failed(deviceId, kind, reason):
        return BridgethingOtaEvent(
            kind: .failed,
            updatedAt: nil, reason: reason, deviceId: deviceId,
            otaKind: kind == .image ? .image : .daemon,
            fromVersion: nil, toVersion: nil, phase: nil, percent: nil,
            deviceChannel: nil, configuredChannel: nil
        )
    }
}
