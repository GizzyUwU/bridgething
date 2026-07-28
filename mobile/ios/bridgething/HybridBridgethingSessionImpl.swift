import BridgethingCompanion
import BridgethingGateway
import BridgethingGlue
import BridgethingLyrics
import BridgethingSchema
import BridgethingSession
import CryptoKit
import Foundation
import NitroModules
import React
import UIKit

private final class ReloadDetacher: NSObject, RCTReloadListener {
    private let onReload: () -> Void
    init(onReload: @escaping () -> Void) {
        self.onReload = onReload
        super.init()
    }

    func didReceiveReloadCommand() {
        onReload()
    }
}

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
    private var foreground = false
    private var foregroundGen: UInt64 = 0
    private var webappsGen: [String: UInt64] = [:]
    private var lifecycleObservers: [NSObjectProtocol] = []
    private var reloadDetacher: ReloadDetacher?
    private var companion: BridgethingCompanion?
    private var eventsTask: Task<Void, Never>?
    private var otaEventsTask: Task<Void, Never>?
    private var deviceMetaTask: Task<Void, Never>?
    private var webappDocTask: Task<Void, Never>?
    private var peers: [String: BridgethingSessionPeer] = [:]
    private var lastNowPlaying: BridgethingNowPlaying?
    private var connectTasks: [String: Task<Void, Never>] = [:]
    private var authStates: [String: BridgethingAuthState] = [:]
    private var healthStates: [String: BridgethingServiceHealth] = [:]
    private var connectedIds: Set<String> = []
    private var priority: [String] = []

    private var onProvidersChanged: (@Sendable ([BridgethingProviderInfo]) -> Void)?
    private var onPeerConnected: (@Sendable (BridgethingSessionPeer) -> Void)?
    private var onPeerDisconnected: (@Sendable (String) -> Void)?
    private var onPeerLinkFailed: (@Sendable (BridgethingSessionPeer) -> Void)?
    private var onNowPlayingChanged: (@Sendable (BridgethingNowPlaying?) -> Void)?
    private var onAncsAuthStatusChanged: (@Sendable (String, BridgethingAncsAuthStatus) -> Void)?
    private var onLog: (@Sendable (String, String) -> Void)?
    private var onWebappsChanged: (@Sendable (BridgethingDeviceWebappsEntry) -> Void)?
    private var onWebappDocChanged: (@Sendable (String, String, String, String?) -> Void)?
    private var onDeviceMetaChanged: (@Sendable (String, BridgethingDeviceMeta) -> Void)?
    private var onOtaRunChanged: (@Sendable (BridgethingOtaRun) -> Void)?
    private var onOtaAvailableChanged: (@Sendable (BridgethingOtaAvailable) -> Void)?
    private var onOtaPollChanged: (@Sendable (BridgethingOtaPollStatus) -> Void)?
    private var onResumed: (@Sendable (BridgethingSessionSnapshot) -> Void)?
    private var logStreamingDesired: Bool = false
    private var localLogStreamingDesired: Bool = false

    public init() {
        observeAppLifecycle()
        registerReloadDetach()
    }

    deinit {
        let center = NotificationCenter.default
        for token in lifecycleObservers { center.removeObserver(token) }
    }

    private func observeAppLifecycle() {
        let center = NotificationCenter.default
        let active = center.addObserver(
            forName: UIApplication.didBecomeActiveNotification, object: nil, queue: .main
        ) { [weak self] _ in self?.setForeground(true) }
        let background = center.addObserver(
            forName: UIApplication.didEnterBackgroundNotification, object: nil, queue: .main
        ) { [weak self] _ in self?.setForeground(false) }
        lifecycleObservers = [active, background]
    }

    private func setForeground(_ value: Bool) {
        if !value {
            stateLock.withLock {
                foreground = false
                foregroundGen &+= 1
            }
            return
        }
        let gen = stateLock.withLock { () -> UInt64 in
            foregroundGen &+= 1
            return foregroundGen
        }
        Task { [weak self] in
            guard let self else { return }
            let snapshot = await self.snapshot()
            let callback = self.stateLock.withLock { () -> (@Sendable (BridgethingSessionSnapshot) -> Void)? in
                guard self.foregroundGen == gen else { return nil }
                self.foreground = true
                return self.onResumed
            }
            callback?(snapshot)
        }
    }

    private func registerReloadDetach() {
        let detacher = ReloadDetacher { [weak self] in self?.detachObservers() }
        reloadDetacher = detacher
        RCTRegisterReloadCommandListener(detacher)
    }

    private func detachObservers() {
        stateLock.withLock {
            onProvidersChanged = nil
            onPeerConnected = nil
            onPeerDisconnected = nil
            onPeerLinkFailed = nil
            onNowPlayingChanged = nil
            onAncsAuthStatusChanged = nil
            onLog = nil
            onWebappsChanged = nil
            onWebappDocChanged = nil
            onDeviceMetaChanged = nil
            onOtaRunChanged = nil
            onOtaAvailableChanged = nil
            onOtaPollChanged = nil
            onResumed = nil
        }
    }

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
        await companion.setAncsAuthStateObserver { [weak self] deviceId, state in
            self?.emitAncsAuthStatus(deviceId, toRNAncsAuthStatus(state))
        }
        let (deviceDesired, localDesired) = stateLock.withLock { (logStreamingDesired, localLogStreamingDesired) }
        await reconcileLogObserver(companion)
        if deviceDesired { await companion.setDeviceLogStreaming(true) }
        if localDesired { await companion.setLocalLogStreaming(true) }

        try await companion.start()

        let events = companion.gateway.events
        let task = Task { [weak self] in
            for await event in events {
                self?.handleGatewayEvent(event)
            }
        }
        let ota = await companion.ota
        let otaStream = ota.storeChanges
        let otaTask = Task { [weak self] in
            for await change in otaStream {
                self?.emitOtaStoreChange(change)
            }
        }
        let metaStream = ota.metaChanged
        let metaTask = Task { [weak self] in
            for await (deviceId, meta) in metaStream {
                guard let self else { return }
                self.emitDeviceMetaChanged(deviceId, Self.toRNDeviceMeta(meta))
                if let cleared = ota.noteRunMeta(
                    deviceId: deviceId,
                    daemonVersion: meta.appVersion,
                    imageVersion: meta.imageVersion
                ) {
                    self.emitOtaStoreChange(.run(cleared))
                }
            }
        }
        let docStream = companion.gateway.webapp.docChanged
        let docTask = Task { [weak self] in
            for await (deviceId, msg) in docStream {
                self?.emitWebappDocChanged(deviceId, msg.id.uuidString.lowercased(), msg.key, msg.value)
            }
        }
        stateLock.lock()
        eventsTask = task
        otaEventsTask = otaTask
        deviceMetaTask = metaTask
        webappDocTask = docTask
        stateLock.unlock()

        await applyOtaPollConfig(Self.loadOtaPollConfig())
        await applyDeviceAutoResume()

        let order = Self.defaults.stringArray(forKey: PrefKey.providerPriority) ?? []
        stateLock.withLock { priority = order }
        await companion.setProviderPriority(order)

        var restore = Set(Self.defaults.stringArray(forKey: PrefKey.connectedProviders) ?? [])
        for reg in Self.registry where reg.available && reg.hasCredentials() {
            restore.insert(reg.id)
        }
        for reg in Self.registry where reg.available && restore.contains(reg.id) {
            try? await connectProvider(id: reg.id)
        }
    }

    public func stop() async {
        stateLock.lock()
        let auth = Array(connectTasks.values)
        connectTasks.removeAll()
        let events = eventsTask
        let ota = otaEventsTask
        let deviceMeta = deviceMetaTask
        let webappDoc = webappDocTask
        let companion = self.companion
        self.companion = nil
        eventsTask = nil
        otaEventsTask = nil
        deviceMetaTask = nil
        webappDocTask = nil
        stateLock.unlock()

        for task in auth { task.cancel() }
        events?.cancel()
        ota?.cancel()
        deviceMeta?.cancel()
        webappDoc?.cancel()

        await companion?.stop()

        stateLock.lock()
        peers.removeAll()
        lastNowPlaying = nil
        stateLock.unlock()
        emitNowPlaying(nil)
    }

    // MARK: - Provider selection

    public func availableProviders() async -> [BridgethingProviderInfo] {
        providerInfos()
    }

    public func connectProvider(id: String) async throws {
        stateLock.lock(); let prevTask = connectTasks[id]; stateLock.unlock()
        prevTask?.cancel()

        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            let task = Task { [weak self] in
                guard let self else {
                    continuation.resume(throwing: SessionError.deallocated)
                    return
                }
                do {
                    try await runConnect(id: id)
                    continuation.resume()
                } catch is CancellationError {
                    setAuthState(id, .idleState())
                    continuation.resume(throwing: SessionError.cancelled)
                } catch {
                    setAuthState(id, .failed(message: String(describing: error)))
                    continuation.resume(throwing: error)
                }
            }
            stateLock.lock(); connectTasks[id] = task; stateLock.unlock()
        }
    }

    public func cancelAuth(id: String) async {
        stateLock.lock()
        let task = connectTasks.removeValue(forKey: id)
        connectedIds.remove(id)
        stateLock.unlock()
        task?.cancel()
        persistConnected()
        let companion = stateLock.withLock { self.companion }
        await companion?.detach(id: id)
        setAuthState(id, .idleState())
    }

    public func disconnectProvider(id: String) async {
        stateLock.lock()
        let task = connectTasks.removeValue(forKey: id)
        connectedIds.remove(id)
        healthStates.removeValue(forKey: id)
        stateLock.unlock()
        task?.cancel()

        persistConnected()
        Self.registry.first { $0.id == id }?.signOut()

        let companion = stateLock.withLock { self.companion }
        await companion?.detach(id: id)
        setAuthState(id, .idleState())
    }

    public func setProviderPriority(ids: [String]) async {
        stateLock.withLock { priority = ids }
        Self.defaults.set(ids, forKey: PrefKey.providerPriority)
        let companion = stateLock.withLock { self.companion }
        await companion?.setProviderPriority(ids)
        emitProviders()
    }

    private func providerInfos() -> [BridgethingProviderInfo] {
        let (connected, auth, health, order) = stateLock.withLock {
            (connectedIds, authStates, healthStates, priority)
        }
        let infos = Self.registry.map { reg in
            BridgethingProviderInfo(
                id: reg.id,
                displayName: reg.displayName,
                available: reg.available,
                connected: connected.contains(reg.id),
                authState: auth[reg.id] ?? .idleState(),
                serviceHealth: health[reg.id] ?? toRNServiceHealth(.ok)
            )
        }
        return infos.sorted { a, b in
            let ra = order.firstIndex(of: a.id) ?? Int.max
            let rb = order.firstIndex(of: b.id) ?? Int.max
            return ra == rb ? a.id < b.id : ra < rb
        }
    }

    public func snapshot() async -> BridgethingSessionSnapshot {
        let companion = stateLock.withLock { self.companion }
        let libraryProvider = await companion?.libraryGlue().map { type(of: $0).name }
        var ancsStatuses: [BridgethingAncsAuthStatusEntry] = []
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
                ancsStatuses.append(
                    BridgethingAncsAuthStatusEntry(
                        deviceId: id,
                        status: toRNAncsAuthStatus(await companion.currentAncsAuthState(deviceId: id))
                    )
                )
            }
        }

        let (peerList, nowPlaying, order) = stateLock.withLock {
            (Array(self.peers.values), self.lastNowPlaying, self.priority)
        }

        var webappEntries: [BridgethingDeviceWebappsEntry] = []
        for peer in peerList where peer.status == .connected {
            if let entry = await webappsEntry(deviceId: peer.id) { webappEntries.append(entry) }
        }

        let ota = await companion?.ota

        return BridgethingSessionSnapshot(
            hostInfo: rnHostInfo(),
            providers: providerInfos(),
            providerPriority: order,
            libraryProvider: libraryProvider,
            peers: peerList,
            ancsAuthStatuses: ancsStatuses,
            nowPlaying: nowPlaying,
            deviceMeta: deviceMetaEntries,
            capabilityFlags: Self.loadCapabilityFlags(),
            otaPollConfig: Self.loadOtaPollConfig(),
            webapps: webappEntries,
            otaRuns: (ota?.retainedRuns() ?? []).map(toRNOtaRun),
            otaAvailable: (ota?.retainedAvailable() ?? []).map(toRNOtaAvailable),
            otaPoll: toRNOtaPollStatus(ota?.retainedPollStatus() ?? OtaPollStatus())
        )
    }

    public func dismissOtaRun(deviceId: String) async {
        guard let companion = stateLock.withLock({ self.companion }) else { return }
        await companion.ota.dismissRun(deviceId: deviceId)
    }

    public func deviceLogSnapshot(limit: Double) async -> [BridgethingDeviceLogLine] {
        DeviceLogRing.shared.tail(limit: Int(limit)).map {
            BridgethingDeviceLogLine(seq: Double($0.seq), ts: $0.timestampMs, level: $0.level, message: $0.message)
        }
    }

    public func persistedLogSize() async -> Double { 0 }

    public func logArchives() async -> [BridgethingLogArchive] { [] }

    public func exportLogs(archiveId: String?) async throws -> String {
        throw SessionError.unsupportedOnPlatform
    }

    public func shareLogs(archiveId: String?) async -> Bool { false }

    public func deleteLogArchive(archiveId: String) async {}

    public func clearPersistedLogs() async {}

    public func companionDebug() async -> BridgethingCompanionDebug {
        let companion = stateLock.withLock { self.companion }
        var glue: (any BridgethingGlue)?
        if let companion {
            glue = await companion.audibleGlue()
            if glue == nil { glue = await companion.libraryGlue() }
        }
        let debug = await glue?.debugState() ?? GlueDebugState()
        return BridgethingCompanionDebug(
            authorityPlaybackHeld: debug.authorityPlaybackHeld,
            authorityMetadataHeld: debug.authorityMetadataHeld
        )
    }

    // MARK: - ANCS

    public func enableAncsNotifications(deviceId: String) async -> BridgethingAncsSetupResult {
        let companion = stateLock.withLock { self.companion }
        guard let companion else {
            return BridgethingAncsSetupResult(
                kind: .failed,
                authStatus: .unknown,
                message: "session not started"
            )
        }
        let result = await companion.enableAncsNotifications(deviceId: deviceId)
        return toRNAncsSetupResult(result)
    }

    public func ancsAuthStatus(deviceId: String) async -> BridgethingAncsAuthStatus {
        let companion = stateLock.withLock { self.companion }
        guard let companion else { return .unknown }
        return toRNAncsAuthStatus(await companion.currentAncsAuthState(deviceId: deviceId))
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

        let ota = await companion.ota
        let result = await ota.installWebapp(
            gateway: companion.gateway,
            deviceId: deviceId,
            bundlePath: archiveUrl,
            provenance: Self.provenanceForSideload(sourceUri)
        )
        switch result {
        case let .installed(info):
            emitWebappsChanged(deviceId)
            return Self.toRNWebappInfo(info)
        case let .failed(reason):
            throw SessionError.installFailed(reason)
        }
    }

    private static func provenanceForSideload(_ sourceUri: String) -> String? {
        guard let url = URL(string: sourceUri),
              let scheme = url.scheme?.lowercased(),
              scheme == "http" || scheme == "https"
        else { return nil }
        return sourceUri
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

    public func getWebappSlots(deviceId: String) async throws -> BridgethingWebappSlots {
        let companion = try requirePeerConnected(deviceId)
        let result = try await companion.gateway.webapp.getSlots(deviceId: deviceId)
        return Self.toRNWebappSlots(try unwrapVoidErr(result, label: "getWebappSlots"))
    }

    public func setWebappSlot(deviceId: String, slot: BridgethingWebappSlot, id: String?) async throws
        -> BridgethingWebappSlots
    {
        let companion = try requirePeerConnected(deviceId)
        let req = WebappSetSlot(slot: slot == .launcher ? .launcher : .overlay, id: try id.map(parseUuid))
        let result = try await companion.gateway.webapp.setSlot(deviceId: deviceId, req)
        let slots = try unwrapWebappErr(result, label: "setWebappSlot")
        emitWebappsChanged(deviceId)
        return Self.toRNWebappSlots(slots)
    }

    public func webappIcon(deviceId: String, id: String) async throws -> BridgethingWebappIcon? {
        let uuid = try parseUuid(id)
        let companion = try requirePeerConnected(deviceId)
        do {
            let resolved = try await companion.webappResources.fetch(deviceId: deviceId, webappId: uuid, kind: .icon)
            if resolved.mime == "image/svg+xml", let svg = try? String(contentsOf: resolved.url, encoding: .utf8) {
                return BridgethingWebappIcon(fileUri: nil, svg: svg, mime: resolved.mime)
            }
            return BridgethingWebappIcon(fileUri: resolved.url.absoluteString, svg: nil, mime: resolved.mime)
        } catch let WebappResourceError.domain(err) {
            if case .resourceNotAvailable = err { return nil }
            throw SessionError.webappError(err)
        } catch let WebappResourceError.wire(err) {
            throw SessionError.protocolError(err)
        }
    }

    public func webappSettingsPage(deviceId: String, id: String) async throws -> String {
        let uuid = try parseUuid(id)
        let companion = try requirePeerConnected(deviceId)
        do {
            let resolved = try await companion.webappResources.fetch(deviceId: deviceId, webappId: uuid, kind: .settings)
            return resolved.url.absoluteString
        } catch let WebappResourceError.domain(err) {
            throw SessionError.webappError(err)
        } catch let WebappResourceError.wire(err) {
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

    public func getWebappDoc(deviceId: String, id: String, key: String) async throws -> String? {
        let uuid = try parseUuid(id)
        let companion = try requirePeerConnected(deviceId)
        let req = WebappDocGet(id: uuid, key: key)
        let result = try await companion.gateway.webapp.docGet(deviceId: deviceId, req)
        return try unwrapWebappErr(result, label: "getWebappDoc").value
    }

    public func listWebappDoc(deviceId: String, id: String) async throws -> [BridgethingDocEntry] {
        let uuid = try parseUuid(id)
        let companion = try requirePeerConnected(deviceId)
        let req = WebappDocList(id: uuid)
        let result = try await companion.gateway.webapp.docList(deviceId: deviceId, req)
        return try unwrapWebappErr(result, label: "listWebappDoc").entries.map {
            BridgethingDocEntry(key: $0.key, value: $0.value)
        }
    }

    public func setWebappDoc(deviceId: String, id: String, key: String, value: String) async throws {
        let uuid = try parseUuid(id)
        let companion = try requirePeerConnected(deviceId)
        let req = WebappDocSet(id: uuid, key: key, value: value)
        let result = try await companion.gateway.webapp.docSet(deviceId: deviceId, req)
        _ = try unwrapWebappErr(result, label: "setWebappDoc")
    }

    public func deleteWebappDoc(deviceId: String, id: String, key: String) async throws {
        let uuid = try parseUuid(id)
        let companion = try requirePeerConnected(deviceId)
        let req = WebappDocDelete(id: uuid, key: key)
        let result = try await companion.gateway.webapp.docDelete(deviceId: deviceId, req)
        _ = try unwrapWebappErr(result, label: "deleteWebappDoc")
    }

    // MARK: - Capability flags

    public func setCapabilityFlags(flags: BridgethingCapabilityFlags) async {
        Self.saveCapabilityFlags(flags)
        let companion = stateLock.withLock { self.companion }
        await companion?.setCapabilityFlags(Self.toCompanionFlags(flags))
    }

    // MARK: - OTA

    public func setDeviceAutoResume(deviceId: String, enabled: Bool) async {
        var map = Self.loadAutoResumeMap()
        map[deviceId] = enabled
        Self.defaults.set(map, forKey: PrefKey.autoResume)
        let companion = stateLock.withLock { self.companion }
        await companion?.setDeviceAutoResume(deviceId: deviceId, enabled: enabled)
    }

    public func isDeviceAutoResumeEnabled(deviceId: String) async -> Bool {
        Self.loadAutoResumeMap()[deviceId] ?? true
    }

    private func applyDeviceAutoResume() async {
        let companion = stateLock.withLock { self.companion }
        guard let companion else { return }
        for (deviceId, enabled) in Self.loadAutoResumeMap() {
            await companion.setDeviceAutoResume(deviceId: deviceId, enabled: enabled)
        }
    }

    private static func loadAutoResumeMap() -> [String: Bool] {
        defaults.dictionary(forKey: PrefKey.autoResume) as? [String: Bool] ?? [:]
    }

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
                intervalSeconds: max(60, config.intervalSeconds),
                cacheDirectory: nil,
                autoPush: config.autoPush
            )
            await ota?.setPollConfig(mapped)
        } else {
            await ota?.setPollConfig(nil)
        }
    }

    public func checkForOtaUpdate(rootUrl: String?) async {
        let companion = stateLock.withLock { self.companion }
        let ota = await companion?.ota
        await ota?.checkNow(rootURL: Self.otaRootURL(rootUrl))
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

    // MARK: - Webapp install

    public func installWebappFromUrl(
        deviceId: String,
        url: String,
        sha256: String,
        size: Double,
        provenance: String?,
        webappId: String?,
        webappName: String?
    ) async throws -> BridgethingWebappInfo {
        let companion = try requirePeerConnected(deviceId)
        guard let parsed = URL(string: url), size >= 0 else { throw SessionError.invalidArchive }
        let result = await companion.ota.installWebappFromUrl(
            gateway: companion.gateway,
            deviceId: deviceId,
            url: parsed,
            sha256: sha256.lowercased(),
            size: UInt64(size),
            provenance: provenance,
            webappId: webappId,
            webappName: webappName
        )
        switch result {
        case let .installed(info):
            emitWebappsChanged(deviceId)
            return Self.toRNWebappInfo(info)
        case let .failed(reason):
            throw SessionError.installFailed(reason)
        }
    }

    private static func jsonString<T: Encodable>(_ value: T) -> String? {
        guard let data = try? JSONEncoder().encode(value) else { return nil }
        return String(data: data, encoding: .utf8)
    }

    public func reconnectPeer(deviceId: String) async throws {
        let companion = stateLock.withLock { self.companion }
        guard let companion else { return }
        try await companion.gateway.reconnect(deviceId: deviceId)
    }

    public func deviceSetNickname(deviceId: String, nickname: String) async throws {
        let companion = try requirePeerConnected(deviceId)
        let result = try await companion.gateway.system.deviceSetNickname(
            deviceId: deviceId, DeviceSetNickname(nickname: nickname)
        )
        switch result {
        case .ok:
            return
        case let .domain(rejected):
            throw SessionError.nicknameRejected(rejected.reason)
        case let .protocolError(err):
            throw SessionError.protocolError(err)
        }
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
        static let autoResume = "bridgething.autoresume"
        static let otaConfigured = "bridgething.ota.configured"
        static let otaInterval = "bridgething.ota.intervalSeconds"
        static let otaAutoPush = "bridgething.ota.autoPush"
        static let otaRootUrl = "bridgething.ota.rootUrl"
        static let connectedProviders = "bridgething.connectedProviders"
        static let providerPriority = "bridgething.providerPriority"
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
        guard defaults.bool(forKey: PrefKey.otaConfigured) else {
            return BridgethingOtaPollConfig(intervalSeconds: 3600, autoPush: true, rootUrl: nil)
        }
        let root = defaults.string(forKey: PrefKey.otaRootUrl)
        return BridgethingOtaPollConfig(
            intervalSeconds: defaults.double(forKey: PrefKey.otaInterval),
            autoPush: defaults.object(forKey: PrefKey.otaAutoPush) == nil ? true : defaults.bool(forKey: PrefKey.otaAutoPush),
            rootUrl: (root?.isEmpty == false) ? root : nil
        )
    }

    private static func saveOtaPollConfig(_ config: BridgethingOtaPollConfig?) {
        guard let config else {
            defaults.set(false, forKey: PrefKey.otaConfigured)
            return
        }
        defaults.set(true, forKey: PrefKey.otaConfigured)
        defaults.set(config.intervalSeconds, forKey: PrefKey.otaInterval)
        defaults.set(config.autoPush, forKey: PrefKey.otaAutoPush)
        defaults.set(config.rootUrl, forKey: PrefKey.otaRootUrl)
    }

    // MARK: - Callback setters

    public func setOnProvidersChanged(_ callback: @escaping @Sendable ([BridgethingProviderInfo]) -> Void) {
        stateLock.withLock { onProvidersChanged = callback }
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

    public func setOnAncsAuthStatusChanged(_ callback: @escaping @Sendable (String, BridgethingAncsAuthStatus) -> Void) {
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
        Task { [weak self] in
            await self?.reconcileLogObserver(companion)
            await companion.setDeviceLogStreaming(enabled)
        }
    }

    public func setLocalLogStreamingEnabled(_ enabled: Bool) {
        let companion: BridgethingCompanion? = stateLock.withLock {
            localLogStreamingDesired = enabled
            return self.companion
        }
        guard let companion else { return }
        Task { [weak self] in
            await self?.reconcileLogObserver(companion)
            await companion.setLocalLogStreaming(enabled)
        }
    }

    private func reconcileLogObserver(_ companion: BridgethingCompanion) async {
        let wantObserver = stateLock.withLock { logStreamingDesired || localLogStreamingDesired }
        if wantObserver {
            await companion.setLogObserver { [weak self] level, message in
                self?.emitLog(level.rawValue, message)
            }
        } else {
            await companion.setLogObserver(nil)
        }
    }

    public func setOnWebappsChanged(_ callback: @escaping @Sendable (BridgethingDeviceWebappsEntry) -> Void) {
        stateLock.withLock { onWebappsChanged = callback }
    }

    public func setOnWebappDocChanged(_ callback: @escaping @Sendable (String, String, String, String?) -> Void) {
        stateLock.withLock { onWebappDocChanged = callback }
    }

    public func setOnDeviceMetaChanged(_ callback: @escaping @Sendable (String, BridgethingDeviceMeta) -> Void) {
        stateLock.withLock { onDeviceMetaChanged = callback }
    }


    public func setOnOtaRunChanged(_ callback: @escaping @Sendable (BridgethingOtaRun) -> Void) {
        stateLock.withLock { onOtaRunChanged = callback }
    }

    public func setOnOtaAvailableChanged(_ callback: @escaping @Sendable (BridgethingOtaAvailable) -> Void) {
        stateLock.withLock { onOtaAvailableChanged = callback }
    }

    public func setOnOtaPollChanged(_ callback: @escaping @Sendable (BridgethingOtaPollStatus) -> Void) {
        stateLock.withLock { onOtaPollChanged = callback }
    }

    public func setOnResumed(_ callback: @escaping @Sendable (BridgethingSessionSnapshot) -> Void) {
        stateLock.withLock { onResumed = callback }
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
    public func forgetCompanionDevice(mac: String) async throws {}
    public func isIgnoringBatteryOptimizations() async -> Bool { false }
    public func requestIgnoreBatteryOptimizations() async throws { throw SessionError.unsupportedOnPlatform }
    public func revokeRuntimePermissions(permissions: [String]) async -> Bool { false }
    public func killApp() async {
        // apple rejects explicit process termination.
    }

    // MARK: - Internal

    private func runConnect(id: String) async throws {
        let companion = stateLock.withLock { self.companion }
        guard let companion else { throw SessionError.notStarted }
        guard let registration = Self.registry.first(where: { $0.id == id }) else {
            throw SessionError.unknownProvider(id)
        }

        let glue = registration.factory()
        await glue.setAuthObserver { [weak self] state in
            self?.handleGlueAuthState(id, state)
        }
        await glue.setServiceHealthObserver { [weak self] health in
            self?.setServiceHealth(id, toRNServiceHealth(health))
        }
        try await companion.attach(glue)
        try Task.checkCancellation()
    }

    private func handleGlueAuthState(_ id: String, _ state: GlueAuthState) {
        switch state {
        case let .pending(prompt):
            setAuthState(id, .pendingState(
                userCode: prompt?.userCode,
                verificationUrl: prompt?.verificationURL.absoluteString,
                verificationUrlComplete: prompt?.verificationURLComplete.absoluteString
            ))
        case .authenticated:
            stateLock.withLock { _ = connectedIds.insert(id) }
            persistConnected()
            setAuthState(id, .authenticated())
        case let .failed(message):
            setAuthState(id, .failed(message: message))
        }
    }

    private func setAuthState(_ id: String, _ state: BridgethingAuthState) {
        stateLock.withLock { authStates[id] = state }
        emitProviders()
    }

    private func setServiceHealth(_ id: String, _ health: BridgethingServiceHealth) {
        stateLock.withLock { healthStates[id] = health }
        emitProviders()
    }

    private func persistConnected() {
        let ids = stateLock.withLock { Array(connectedIds).sorted() }
        Self.defaults.set(ids, forKey: PrefKey.connectedProviders)
    }

    private func handleGatewayEvent(_ event: GatewayEvent) {
        switch event {
        case let .connected(device):
            let peer = BridgethingSessionPeer(id: device.id, name: device.name, status: .connected, linkError: nil)
            stateLock.withLock { peers[device.id] = peer }
            emitPeerConnected(peer)
        case let .disconnected(id):
            stateLock.withLock {
                _ = peers.removeValue(forKey: id)
                _ = webappsGen.removeValue(forKey: id)
            }
            emitPeerDisconnected(id)
        case let .linkFailed(device, reason):
            let peer = BridgethingSessionPeer(id: device.id, name: device.name, status: .linkfailed, linkError: reason)
            stateLock.withLock { peers[device.id] = peer }
            emitPeerLinkFailed(peer)
        case .message:
            break
        case let .decodeError(id, description):
            emitLog("warn", "[\(id)] decode error: \(description)")
        }
    }

    private func handleNowPlaying(_ glue: GlueNowPlaying?) {
        let rn: BridgethingNowPlaying? = glue.flatMap(Self.toRNNowPlaying)
        stateLock.withLock { lastNowPlaying = rn }
        emitNowPlaying(rn)
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

    // MARK: - Emit helpers

    private func emitProviders() {
        let cb = stateLock.withLock { foreground ? onProvidersChanged : nil }
        cb?(providerInfos())
    }

    private func emitPeerConnected(_ peer: BridgethingSessionPeer) {
        stateLock.withLock { foreground ? onPeerConnected : nil }?(peer)
    }

    private func emitPeerDisconnected(_ id: String) {
        stateLock.withLock { foreground ? onPeerDisconnected : nil }?(id)
    }

    private func emitPeerLinkFailed(_ peer: BridgethingSessionPeer) {
        stateLock.withLock { foreground ? onPeerLinkFailed : nil }?(peer)
    }

    private func emitNowPlaying(_ np: BridgethingNowPlaying?) {
        stateLock.withLock { foreground ? onNowPlayingChanged : nil }?(np)
    }

    private func emitAncsAuthStatus(_ deviceId: String, _ status: BridgethingAncsAuthStatus) {
        stateLock.withLock { foreground ? onAncsAuthStatusChanged : nil }?(deviceId, status)
    }

    private func emitLog(_ level: String, _ message: String) {
        stateLock.withLock { foreground ? onLog : nil }?(level, message)
    }

    private func emitWebappsChanged(_ deviceId: String) {
        let gen = stateLock.withLock { () -> UInt64 in
            let next = (webappsGen[deviceId] ?? 0) &+ 1
            webappsGen[deviceId] = next
            return next
        }
        Task { [weak self] in
            guard let self else { return }
            for attempt in 1...Self.webappsReadAttempts {
                let superseded = self.stateLock.withLock { self.webappsGen[deviceId] != gen }
                if superseded { return }
                if let entry = await self.webappsEntry(deviceId: deviceId) {
                    let cb = self.stateLock.withLock { () -> (@Sendable (BridgethingDeviceWebappsEntry) -> Void)? in
                        guard self.webappsGen[deviceId] == gen, self.foreground else { return nil }
                        return self.onWebappsChanged
                    }
                    cb?(entry)
                    return
                }
                if attempt < Self.webappsReadAttempts {
                    try? await Task.sleep(for: .milliseconds(400 * attempt))
                }
            }
        }
    }

    private static let webappsReadAttempts = 3

    private func webappsEntry(deviceId: String) async -> BridgethingDeviceWebappsEntry? {
        guard let list = try? await listWebapps(deviceId: deviceId) else { return nil }
        let active = try? await currentWebapp(deviceId: deviceId)
        return BridgethingDeviceWebappsEntry(deviceId: deviceId, webapps: list, active: active ?? nil)
    }

    private func emitOtaStoreChange(_ change: OtaStoreChange) {
        switch change {
        case let .run(run):
            stateLock.withLock { foreground ? onOtaRunChanged : nil }?(toRNOtaRun(run))
        case let .available(available):
            stateLock.withLock { foreground ? onOtaAvailableChanged : nil }?(toRNOtaAvailable(available))
        case let .poll(status):
            stateLock.withLock { foreground ? onOtaPollChanged : nil }?(toRNOtaPollStatus(status))
        }
    }

    private func emitWebappDocChanged(_ deviceId: String, _ webappId: String, _ key: String, _ value: String?) {
        stateLock.withLock { foreground ? onWebappDocChanged : nil }?(deviceId, webappId, key, value)
    }

    private func emitDeviceMetaChanged(_ deviceId: String, _ meta: BridgethingDeviceMeta) {
        stateLock.withLock { foreground ? onDeviceMetaChanged : nil }?(deviceId, meta)
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
            libbridgethingVersion: meta.libbridgethingVersion,
            imageVersion: meta.imageVersion,
            appName: meta.appName,
            osName: meta.osName,
            osVersion: meta.osVersion,
            channel: meta.channel,
            modelName: meta.modelName,
            serialNumber: meta.serialNumber,
            nickname: meta.nickname
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
            provenance: info.provenance,
            description: info.description,
            iconHash: info.iconHash,
            settingsHash: info.settingsHash,
            overlayHash: info.overlayHash,
            config: info.config.map(toRNConfigField),
            permissions: info.permissions
        )
    }

    private static func toRNWebappSlots(_ slots: BridgethingSchema.WebappSlots) -> BridgethingWebappSlots {
        BridgethingWebappSlots(
            launcher: slots.launcher?.uuidString.lowercased(),
            overlay: slots.overlay?.uuidString.lowercased()
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
    case installFailed(String)
    case webappError(WebappError)
    case protocolError(WireError)
    case nicknameRejected(String)
    case unsupportedOnPlatform
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



private func rnOtaStepKind(_ k: OtaStepKind) -> BridgethingOtaStepKind {
    switch k {
    case .download: .download
    case .stream: .stream
    case .apply: .apply
    case .reboot: .reboot
    }
}


private func toRNOtaRunPhase(_ p: OtaRunPhase) -> BridgethingOtaPhase {
    switch p {
    case .idle: .idle
    case .downloading: .downloading
    case .streaming: .streaming
    case .verifying: .verifying
    case .writing: .writing
    case .confirming: .confirming
    case .reboot: .reboot
    case .completed: .completed
    case .failed: .failed
    }
}

private func toRNOtaKind(_ k: OtaKind) -> BridgethingOtaKind {
    switch k {
    case .image: .image
    case .daemon: .daemon
    case .builtinWebapp: .builtinwebapp
    case .installedWebapp: .installedwebapp
    }
}

private func toRNOtaOutcome(_ o: OtaRunOutcome) -> BridgethingOtaOutcome {
    switch o {
    case .succeeded: .succeeded
    case .failed: .failed
    case .cancelled: .cancelled
    }
}

func toRNOtaRun(_ run: OtaRun) -> BridgethingOtaRun {
    BridgethingOtaRun(
        runId: run.runId,
        deviceId: run.deviceId,
        otaKind: toRNOtaKind(run.kind),
        phase: toRNOtaRunPhase(run.phase),
        steps: run.steps.map {
            BridgethingOtaStep(id: Double($0.id), kind: rnOtaStepKind($0.kind), label: $0.label, bytes: Double($0.bytes))
        },
        stepId: Double(run.stepId),
        startedAt: run.startedAt.timeIntervalSince1970 * 1000,
        phaseStartedAt: run.phaseStartedAt.timeIntervalSince1970 * 1000,
        stageReceived: run.stageReceived.map(Double.init),
        stageTotal: run.stageTotal.map(Double.init),
        ratePerSec: run.ratePerSec,
        dwlPercent: run.dwlPercent.map(Double.init),
        outcome: run.outcome.map(toRNOtaOutcome),
        error: run.error,
        releaseVersion: run.releaseVersion,
        daemonVersion: run.daemonVersion,
        imageVersion: run.imageVersion,
        webappId: run.webappId,
        webappName: run.webappName
    )
}

func toRNOtaAvailable(_ a: OtaAvailable) -> BridgethingOtaAvailable {
    BridgethingOtaAvailable(
        deviceId: a.deviceId,
        releaseVersion: a.releaseVersion,
        daemonVersion: a.daemonVersion,
        imageVersion: a.imageVersion
    )
}

func toRNOtaPollStatus(_ s: OtaPollStatus) -> BridgethingOtaPollStatus {
    BridgethingOtaPollStatus(lastPolledAt: s.lastPolledAt, error: s.error)
}
