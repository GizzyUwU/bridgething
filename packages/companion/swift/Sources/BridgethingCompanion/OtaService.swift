import BridgethingGateway
import BridgethingSchema
import Foundation

#if canImport(CryptoKit)
    import CryptoKit
#endif
#if canImport(FoundationNetworking)
    import FoundationNetworking
#endif

public enum OtaPhaseSnapshot: Sendable, Equatable {
    case idle
    case downloading(asset: String, received: UInt64, total: UInt64, ratePerSec: Double?)
    case streaming(sent: UInt64, total: UInt64, ratePerSec: Double?, etaSeconds: Double?)
    case rangePull(asset: String, served: UInt64, ratePerSec: Double?)
    case applying(phase: OtaPhase, percent: Int)
    case staged
    case completed
    case failed(reason: String)
}

final class RateTracker: @unchecked Sendable {
    private struct Sample { let bytes: UInt64; let at: Date }
    private var samples: [Sample] = []
    private let window: TimeInterval
    private let lock = NSLock()

    init(window: TimeInterval = 4.0) { self.window = window }

    func record(_ bytes: UInt64) {
        lock.lock(); defer { lock.unlock() }
        let now = Date()
        samples.append(Sample(bytes: bytes, at: now))
        let cutoff = now.addingTimeInterval(-window)
        while samples.count > 2, let first = samples.first, first.at < cutoff {
            samples.removeFirst()
        }
    }

    func ratePerSec() -> Double? {
        lock.lock(); defer { lock.unlock() }
        guard let first = samples.first, let last = samples.last else { return nil }
        let dt = last.at.timeIntervalSince(first.at)
        guard dt > 0.05, last.bytes >= first.bytes else { return nil }
        return Double(last.bytes - first.bytes) / dt
    }

    func etaSeconds(remaining: UInt64) -> Double? {
        guard let rate = ratePerSec(), rate > 0 else { return nil }
        return Double(remaining) / rate
    }
}

public enum WebappInstallResult: Sendable {
    case installed(WebappInfo)
    case failed(reason: String)
}

public struct OtaPollConfig: Sendable, Equatable {
    public var rootURL: URL
    public var channel: String
    public var intervalSeconds: TimeInterval
    public var cacheDirectory: URL?
    public var autoPush: Bool

    public init(
        rootURL: URL = URL(string: "https://ota.bridgething.com")!,
        channel: String,
        intervalSeconds: TimeInterval = 3600,
        cacheDirectory: URL? = nil,
        autoPush: Bool = true
    ) {
        self.rootURL = rootURL
        self.channel = channel
        self.intervalSeconds = intervalSeconds
        self.cacheDirectory = cacheDirectory
        self.autoPush = autoPush
    }
}

public enum OtaPollEvent: Sendable, Equatable {
    case manifestPolled(updatedAt: String)
    case manifestPollFailed(reason: String)
    case channelMismatch(deviceId: String, deviceChannel: String, configuredChannel: String)
    case updateAvailable(deviceId: String, kind: OtaKind, fromVersion: String, toVersion: String)
    case progress(deviceId: String, kind: OtaKind, snapshot: OtaPhaseSnapshot)
    case updated(deviceId: String, kind: OtaKind, version: String)
    case failed(deviceId: String, kind: OtaKind, reason: String)
}

public actor OtaService {
    private var localZcks: [String: URL] = [:]
    private var rangeServerTask: Task<Void, Never>?
    private var metaTask: Task<Void, Never>?
    private var nicknameTask: Task<Void, Never>?
    private var pollTask: Task<Void, Never>?

    private var attachedGateway: BridgethingGateway?
    private var pollConfig: OtaPollConfig?
    private var deviceMeta: [String: BridgeThingMeta] = [:]
    private var inFlight: Set<String> = []
    private var autoPushNextAt: [String: Date] = [:]
    private var autoPushFailures: [String: Int] = [:]
    private var linkOpenAt: [String: Date] = [:]
    private var pollSleep: Task<Void, Never>?

    private var imageProgress: AsyncStream<OtaPhaseSnapshot>.Continuation?
    private var imageProgressDeviceId: String?
    private var rangeServed: [String: UInt64] = [:]
    private var rangeTrackers: [String: RateTracker] = [:]

    nonisolated let transferAcks = TransferAckWindow()

    static let systemZckAsset = "system.img.zck"
    static let bootZckAsset = "boot.vfat.zck"

    private static let otaFragmentBytes: UInt64 = 4 * 1024
    private static let otaWindowBytes: UInt64 = 32 * 1024
    private static let otaAckTimeoutSeconds: Double = 15
    private static let autoPushBackoffBase: TimeInterval = 120
    private static let autoPushBackoffMax: TimeInterval = 15 * 60
    private static let minResumeDelay: TimeInterval = 5
    private static let linkStabilitySeconds: TimeInterval = 120

    private static let builtinWebapps: [(slug: String, id: UUID)] = [
        ("hub", UUID(uuidString: "019693c0-5c6a-71f0-a89d-7e2a4d9c0a01")!),
        ("stock", UUID(uuidString: "b12be731-416c-4cf7-8a91-3d2f19a45e21")!),
    ]

    private struct BandaidPiece {
        let kind: OtaKind
        let url: URL
        let filename: String
        let version: String
        let assetLabel: String
        let expected: OtaArtifactDigest?
    }

    private let eventContinuation: AsyncStream<OtaPollEvent>.Continuation
    private let metaChangedContinuation: AsyncStream<(deviceId: String, meta: BridgeThingMeta)>.Continuation

    public nonisolated let events: AsyncStream<OtaPollEvent>

    public nonisolated let metaChanged: AsyncStream<(deviceId: String, meta: BridgeThingMeta)>

    public init() {
        let (stream, continuation) = AsyncStream.makeStream(of: OtaPollEvent.self)
        events = stream
        eventContinuation = continuation
        let (metaStream, metaContinuation) = AsyncStream.makeStream(of: (deviceId: String, meta: BridgeThingMeta).self)
        metaChanged = metaStream
        metaChangedContinuation = metaContinuation
    }

    public func start(gateway: BridgethingGateway) async {
        attachedGateway = gateway
        rangeServerTask?.cancel()
        rangeServerTask = Task { [weak self] in
            for await (handle, req) in gateway.system.otaAssetRangeRequests {
                Task { [weak self] in await self?.handleRangeRequest(gateway: gateway, handle: handle, req: req) }
            }
        }
        metaTask?.cancel()
        metaTask = Task { [weak self] in
            for await event in gateway.events {
                guard let self else { return }
                switch event {
                case let .connected(device):
                    await self.noteLinkOpen(deviceId: device.id)
                    await self.wakePoll()
                case let .disconnected(deviceId):
                    await self.noteLinkClosed(deviceId: deviceId)
                case let .linkFailed(device, _):
                    await self.noteLinkClosed(deviceId: device.id)
                case let .message(deviceId, msg):
                    if case let .version(meta) = msg.data {
                        await self.recordMeta(deviceId: deviceId, meta: meta)
                    }
                default:
                    break
                }
            }
        }
        nicknameTask?.cancel()
        nicknameTask = Task { [weak self] in
            for await (deviceId, reply) in gateway.system.deviceNicknameChanged {
                guard let self else { return }
                await recordNickname(deviceId: deviceId, nickname: reply.nickname)
            }
        }
    }

    public func stop() async {
        rangeServerTask?.cancel()
        rangeServerTask = nil
        metaTask?.cancel()
        metaTask = nil
        nicknameTask?.cancel()
        nicknameTask = nil
        pollTask?.cancel()
        pollTask = nil
        attachedGateway = nil
        deviceMeta.removeAll()
    }

    public func setLocalZcks(_ map: [String: URL]) {
        localZcks = map
    }

    public func currentLocalZcks() -> [String: URL] { localZcks }

    public func pushUpdate(
        gateway: BridgethingGateway,
        deviceId: String,
        swuPath: URL,
        zcks: [String: URL],
        updateUrlBase: String? = nil,
        progress: AsyncStream<OtaPhaseSnapshot>.Continuation
    ) async {
        setLocalZcks(zcks)
        let (result, _) = await driveOta(
            gateway: gateway,
            deviceId: deviceId,
            kind: .image,
            artifactPath: swuPath,
            updateUrlBase: updateUrlBase,
            mode: .full,
            progress: progress
        )
        progress.yield(result)
        progress.finish()
    }

    public func pushDaemon(
        gateway: BridgethingGateway,
        deviceId: String,
        binaryPath: URL,
        progress: AsyncStream<OtaPhaseSnapshot>.Continuation
    ) async {
        let result = await applyBandaidBatch(
            gateway: gateway,
            deviceId: deviceId,
            artifacts: [(kind: .daemon, path: binaryPath)],
            progress: progress
        )
        progress.yield(result)
        progress.finish()
    }

    public func pushBuiltinWebapp(
        gateway: BridgethingGateway,
        deviceId: String,
        bundlePath: URL,
        progress: AsyncStream<OtaPhaseSnapshot>.Continuation
    ) async {
        let result = await applyBandaidBatch(
            gateway: gateway,
            deviceId: deviceId,
            artifacts: [(kind: .builtinWebapp, path: bundlePath)],
            progress: progress
        )
        progress.yield(result)
        progress.finish()
    }

    public func pushBandaidBatch(
        gateway: BridgethingGateway,
        deviceId: String,
        artifacts: [(kind: OtaKind, path: URL)],
        progress: AsyncStream<OtaPhaseSnapshot>.Continuation
    ) async {
        let result = await applyBandaidBatch(
            gateway: gateway,
            deviceId: deviceId,
            artifacts: artifacts,
            progress: progress
        )
        progress.yield(result)
        progress.finish()
    }

    // MARK: - webapp install

    public func installWebapp(
        gateway: BridgethingGateway,
        deviceId: String,
        bundlePath: URL
    ) async -> WebappInstallResult {
        if inFlight.contains(deviceId) {
            return .failed(reason: "another update is already in flight for this device")
        }
        inFlight.insert(deviceId)
        defer { inFlight.remove(deviceId) }

        let totalSize: UInt64
        do {
            let attrs = try FileManager.default.attributesOfItem(atPath: bundlePath.path)
            guard let n = (attrs[.size] as? NSNumber)?.uint64Value else {
                return .failed(reason: "could not stat bundle")
            }
            totalSize = n
        } catch {
            return .failed(reason: "stat bundle failed: \(error.localizedDescription)")
        }
        guard totalSize <= UInt64(UInt32.max) else {
            return .failed(reason: "bundle larger than 4 GiB")
        }

        let sha256: String
        do {
            sha256 = try await hashFile(bundlePath)
        } catch {
            return .failed(reason: "sha256 failed: \(error.localizedDescription)")
        }

        let live = gateway.liveEvents
        let terminalTask = Task<WebappInstallResult, Never> {
            await withTaskGroup(of: WebappInstallResult.self) { group in
                group.addTask {
                    for await event in live {
                        guard case let .message(eventDeviceId, message) = event, eventDeviceId == deviceId else { continue }
                        switch message.data {
                        case let .webapp(.webappInstalled(info)):
                            return .installed(info)
                        case let .system(.otaError(err)):
                            return .failed(reason: "[\(err.code)] \(err.msg)")
                        default:
                            continue
                        }
                    }
                    return .failed(reason: "event stream ended")
                }
                group.addTask {
                    try? await Task.sleep(nanoseconds: 60_000_000_000)
                    return .failed(reason: "install timed out")
                }
                let result = await group.next() ?? .failed(reason: "install interrupted")
                group.cancelAll()
                return result
            }
        }

        let transferId = UUID()
        let begin = OtaBegin(
            kind: .installedWebapp,
            updateId: sha256,
            updateUrlBase: nil,
            transfer: TransferRef(id: transferId, totalSize: UInt32(totalSize), sha256: sha256)
        )
        let beginResult: RequestResult<OtaBeginAck, OtaBeginRejected>
        do {
            beginResult = try await gateway.system.otaBegin(deviceId: deviceId, begin)
        } catch {
            terminalTask.cancel()
            return .failed(reason: "OtaBegin send failed: \(error.localizedDescription)")
        }
        let resumeFromOffset: UInt32
        switch beginResult {
        case let .ok(ack):
            resumeFromOffset = ack.resumeFromOffset
        case let .domain(rej):
            terminalTask.cancel()
            return .failed(reason: "daemon rejected install: \(rej.reason)")
        case let .protocolError(err):
            terminalTask.cancel()
            return .failed(reason: "OtaBegin protocol error: \(err)")
        }

        do {
            try await streamArtifact(
                gateway: gateway,
                deviceId: deviceId,
                transferId: transferId,
                artifactPath: bundlePath,
                startOffset: UInt64(resumeFromOffset),
                totalSize: totalSize,
                progress: nil
            )
        } catch {
            terminalTask.cancel()
            return .failed(reason: "chunk stream failed: \(error.localizedDescription)")
        }

        return await terminalTask.value
    }

    // MARK: - manifest poll loop

    public func meta(deviceId: String) -> BridgeThingMeta? {
        deviceMeta[deviceId]
    }

    public func setPollConfig(_ config: OtaPollConfig?) {
        pollConfig = config
        pollTask?.cancel()
        pollTask = nil
        guard let config else { return }
        pollTask = Task { [weak self] in
            await self?.runPollLoop(config: config)
        }
    }

    public func pollNow() async {
        guard let config = pollConfig, let gateway = attachedGateway else { return }
        await poll(config: config, gateway: gateway)
    }

    public func checkNow(channel: String, rootURL: URL) async {
        guard let gateway = attachedGateway else { return }
        let transient = OtaPollConfig(rootURL: rootURL, channel: channel, autoPush: false)
        await poll(config: transient, gateway: gateway)
    }

    public func discoverManifest(rootURL: URL) async throws -> OtaDiscoverManifest {
        try await fetchManifest(url: rootURL.appendingPathComponent("manifest.json"))
    }

    public func applyVersion(deviceId: String, channel: String, version: String, rootURL: URL) async {
        guard let gateway = attachedGateway else {
            eventContinuation.yield(.failed(deviceId: deviceId, kind: .image, reason: "gateway not attached"))
            return
        }
        guard let composite = OtaCompositeVersion.parse(version) else {
            eventContinuation.yield(.failed(deviceId: deviceId, kind: .image, reason: "'\(version)' is not a composite version"))
            return
        }
        guard let meta = deviceMeta[deviceId] else {
            eventContinuation.yield(.failed(deviceId: deviceId, kind: .image, reason: "device meta not yet known"))
            return
        }
        if inFlight.contains(deviceId) { return }
        let config = OtaPollConfig(rootURL: rootURL, channel: channel)
        let urls = OtaArtifactURLs(
            rootURL: rootURL, channel: channel,
            daemonVersion: composite.daemon, imageVersion: composite.image,
            imageVariant: meta.imageVariant
        )
        let artifacts = (try? await discoverManifest(rootURL: rootURL))?.releases[version]?.artifacts
        if meta.imageVersion != composite.image {
            await runImageAuto(
                deviceId: deviceId, targetVersion: composite.image,
                swuURL: urls.imageSwu, zckURL: urls.imageZck, bootZckURL: urls.imageBootZck,
                artifacts: artifacts, config: config, gateway: gateway
            )
            return
        }
        if meta.appVersion != composite.daemon {
            await runBandaidBatchAuto(
                deviceId: deviceId,
                pieces: [BandaidPiece(
                    kind: .daemon,
                    url: urls.daemonBinary,
                    filename: "daemon-\(channel)-\(composite.daemon)",
                    version: composite.daemon,
                    assetLabel: "daemon",
                    expected: artifacts?.daemon
                )],
                config: config, gateway: gateway
            )
        }
    }

    private func runPollLoop(config: OtaPollConfig) async {
        while !Task.isCancelled {
            if let gateway = attachedGateway {
                await poll(config: config, gateway: gateway)
            }
            await sleepUntilNextWake(config: config)
        }
    }

    private func sleepUntilNextWake(config: OtaPollConfig) async {
        let now = Date()
        var deadline = now.addingTimeInterval(max(config.intervalSeconds, 60))
        if let soonest = autoPushNextAt.values.min(), soonest < deadline {
            deadline = max(soonest, now.addingTimeInterval(Self.minResumeDelay))
        }
        for openedAt in linkOpenAt.values {
            let ready = openedAt.addingTimeInterval(Self.linkStabilitySeconds)
            if ready > now, ready < deadline { deadline = ready }
        }
        let seconds = max(deadline.timeIntervalSince(now), 0)
        let task = Task { _ = try? await Task.sleep(nanoseconds: UInt64(seconds * 1_000_000_000)) }
        pollSleep = task
        await withTaskCancellationHandler { await task.value } onCancel: { task.cancel() }
        pollSleep = nil
    }

    func wakePoll() {
        pollSleep?.cancel()
    }

    private func noteLinkOpen(deviceId: String) {
        linkOpenAt[deviceId] = Date()
    }

    private func noteLinkClosed(deviceId: String) {
        linkOpenAt[deviceId] = nil
    }

    private func linkStable(_ deviceId: String) -> Bool {
        guard let openedAt = linkOpenAt[deviceId] else { return false }
        return Date().timeIntervalSince(openedAt) >= Self.linkStabilitySeconds
    }

    private func recordMeta(deviceId: String, meta: BridgeThingMeta) {
        let isNew = deviceMeta[deviceId] == nil
        deviceMeta[deviceId] = meta
        metaChangedContinuation.yield((deviceId: deviceId, meta: meta))
        if isNew { wakePoll() }
    }

    private func recordNickname(deviceId: String, nickname: String?) {
        guard let meta = deviceMeta[deviceId] else { return }
        let updated = BridgeThingMeta(
            bridgethingVersion: meta.bridgethingVersion,
            libbridgethingVersion: meta.libbridgethingVersion,
            appName: meta.appName,
            nickname: nickname,
            appVersion: meta.appVersion,
            osName: meta.osName,
            osVersion: meta.osVersion,
            osDescription: meta.osDescription,
            btMac: meta.btMac,
            serialNumber: meta.serialNumber,
            fccId: meta.fccId,
            icId: meta.icId,
            modelName: meta.modelName,
            channel: meta.channel,
            imageVariant: meta.imageVariant,
            imageVersion: meta.imageVersion,
            imageBuildId: meta.imageBuildId,
            imageBuildDate: meta.imageBuildDate,
            imageDistro: meta.imageDistro,
            imageMachine: meta.imageMachine,
            discord: meta.discord,
            credits: meta.credits
        )
        deviceMeta[deviceId] = updated
        metaChangedContinuation.yield((deviceId: deviceId, meta: updated))
    }

    private func poll(config: OtaPollConfig, gateway: BridgethingGateway) async {
        let manifestURL = config.rootURL.appendingPathComponent("manifest.json")
        let manifest: OtaDiscoverManifest
        do {
            manifest = try await fetchManifest(url: manifestURL)
        } catch {
            eventContinuation.yield(.manifestPollFailed(reason: error.localizedDescription))
            return
        }
        eventContinuation.yield(.manifestPolled(updatedAt: manifest.updatedAt))

        guard let channel = manifest.channels[config.channel] else {
            eventContinuation.yield(.manifestPollFailed(
                reason: "configured channel '\(config.channel)' not in manifest"
            ))
            return
        }
        guard let composite = OtaCompositeVersion.parse(channel.latest) else {
            eventContinuation.yield(.manifestPollFailed(
                reason: "channel.latest '\(channel.latest)' is not a composite version"
            ))
            return
        }
        let release = manifest.releases[channel.latest]
        if let release, release.yanked != nil || release.deprecated { return }

        let snapshot = deviceMeta
        for (deviceId, meta) in snapshot {
            await reconcileDevice(
                deviceId: deviceId,
                meta: meta,
                latest: composite,
                release: release,
                config: config,
                gateway: gateway
            )
        }
    }

    private func reconcileDevice(
        deviceId: String,
        meta: BridgeThingMeta,
        latest: OtaCompositeVersion,
        release: OtaManifestRelease?,
        config: OtaPollConfig,
        gateway: BridgethingGateway
    ) async {
        if meta.channel != config.channel {
            eventContinuation.yield(.channelMismatch(
                deviceId: deviceId,
                deviceChannel: meta.channel,
                configuredChannel: config.channel
            ))
            return
        }
        if inFlight.contains(deviceId) { return }

        let urls = OtaArtifactURLs(
            rootURL: config.rootURL,
            channel: config.channel,
            daemonVersion: latest.daemon,
            imageVersion: latest.image,
            imageVariant: meta.imageVariant
        )

        if meta.imageVersion != latest.image {
            eventContinuation.yield(.updateAvailable(
                deviceId: deviceId,
                kind: .image,
                fromVersion: meta.imageVersion,
                toVersion: latest.image
            ))
            if config.autoPush, autoPushReady(deviceId) {
                await runImageAuto(
                    deviceId: deviceId,
                    targetVersion: latest.image,
                    swuURL: urls.imageSwu,
                    zckURL: urls.imageZck,
                    bootZckURL: urls.imageBootZck,
                    artifacts: release?.artifacts,
                    config: config,
                    gateway: gateway
                )
            }
            return
        }

        var batch: [BandaidPiece] = []
        if meta.appVersion != latest.daemon {
            eventContinuation.yield(.updateAvailable(
                deviceId: deviceId,
                kind: .daemon,
                fromVersion: meta.appVersion,
                toVersion: latest.daemon
            ))
            batch.append(BandaidPiece(
                kind: .daemon,
                url: urls.daemonBinary,
                filename: "daemon-\(config.channel)-\(latest.daemon)",
                version: latest.daemon,
                assetLabel: "daemon",
                expected: release?.artifacts?.daemon
            ))
        }
        for drift in await builtinWebappDrift(deviceId: deviceId, release: release, config: config, gateway: gateway) {
            eventContinuation.yield(.updateAvailable(
                deviceId: deviceId,
                kind: .builtinWebapp,
                fromVersion: drift.fromVersion,
                toVersion: drift.piece.version
            ))
            batch.append(drift.piece)
        }

        if !batch.isEmpty, config.autoPush, autoPushReady(deviceId) {
            await runBandaidBatchAuto(deviceId: deviceId, pieces: batch, config: config, gateway: gateway)
        }
    }

    private struct WebappDrift {
        let piece: BandaidPiece
        let fromVersion: String
    }

    private func builtinWebappDrift(
        deviceId: String,
        release: OtaManifestRelease?,
        config: OtaPollConfig,
        gateway: BridgethingGateway
    ) async -> [WebappDrift] {
        guard let release, !release.builtinWebapps.isEmpty else { return [] }
        let installed = await installedWebapps(deviceId: deviceId, gateway: gateway)
        var out: [WebappDrift] = []
        for builtin in Self.builtinWebapps {
            guard let available = release.builtinWebapps[builtin.slug],
                  let current = installed[builtin.id],
                  current != available
            else { continue }
            let url = OtaArtifactURLs.builtinWebapp(
                rootURL: config.rootURL,
                channel: config.channel,
                name: builtin.slug,
                version: available
            )
            out.append(WebappDrift(
                piece: BandaidPiece(
                    kind: .builtinWebapp,
                    url: url,
                    filename: "webapp-\(config.channel)-\(builtin.slug)-\(available)",
                    version: available,
                    assetLabel: "webapp: \(builtin.slug)",
                    expected: release.artifacts?.webapps[builtin.slug]
                ),
                fromVersion: current
            ))
        }
        return out
    }

    private func installedWebapps(deviceId: String, gateway: BridgethingGateway) async -> [UUID: String] {
        guard let result = try? await gateway.webapp.list(deviceId: deviceId),
              case let .ok(list) = result
        else { return [:] }
        var map: [UUID: String] = [:]
        for webapp in list.webapps { map[webapp.id] = webapp.version }
        return map
    }

    private func tryBeginInFlight(_ deviceId: String) -> Bool {
        if inFlight.contains(deviceId) { return false }
        inFlight.insert(deviceId)
        return true
    }

    private func autoPushReady(_ deviceId: String) -> Bool {
        guard linkStable(deviceId) else { return false }
        guard let next = autoPushNextAt[deviceId] else { return true }
        return Date() >= next
    }

    private func noteAutoPushResult(_ deviceId: String, failed: Bool) {
        if failed {
            let n = (autoPushFailures[deviceId] ?? 0) + 1
            autoPushFailures[deviceId] = n
            let delay = min(Self.autoPushBackoffBase * pow(2, Double(min(n - 1, 5))), Self.autoPushBackoffMax)
            autoPushNextAt[deviceId] = Date().addingTimeInterval(delay)
        } else {
            autoPushFailures[deviceId] = nil
            autoPushNextAt[deviceId] = nil
        }
    }

    private func runBandaidBatchAuto(
        deviceId: String,
        pieces: [BandaidPiece],
        config: OtaPollConfig,
        gateway: BridgethingGateway
    ) async {
        guard !pieces.isEmpty else { return }
        guard tryBeginInFlight(deviceId) else { return }
        defer { inFlight.remove(deviceId) }
        let cacheDir = effectiveCacheDir(config: config)
        let labelKind: OtaKind = pieces.contains { $0.kind == .daemon } ? .daemon : .builtinWebapp
        let (stream, continuation) = AsyncStream.makeStream(of: OtaPhaseSnapshot.self)
        let forwarder = forwardProgress(stream: stream, deviceId: deviceId, kind: labelKind)
        var artifacts: [(kind: OtaKind, path: URL)] = []
        for piece in pieces {
            do {
                let cached = try await downloadIfNeeded(
                    url: piece.url, into: cacheDir, filename: piece.filename,
                    asset: piece.assetLabel, expected: piece.expected, progress: continuation
                )
                artifacts.append((kind: piece.kind, path: cached))
            } catch {
                let reason = "bandaid download failed: \(error.localizedDescription)"
                continuation.yield(.failed(reason: reason))
                continuation.finish()
                _ = await forwarder.value
                eventContinuation.yield(.failed(deviceId: deviceId, kind: piece.kind, reason: reason))
                noteAutoPushResult(deviceId, failed: true)
                return
            }
        }
        await pushBandaidBatch(gateway: gateway, deviceId: deviceId, artifacts: artifacts, progress: continuation)
        let terminal = await forwarder.value
        if case let .failed(reason) = terminal {
            eventContinuation.yield(.failed(deviceId: deviceId, kind: labelKind, reason: reason))
            noteAutoPushResult(deviceId, failed: true)
        } else {
            for piece in pieces {
                eventContinuation.yield(.updated(deviceId: deviceId, kind: piece.kind, version: piece.version))
            }
            noteAutoPushResult(deviceId, failed: false)
        }
    }

    private func runImageAuto(
        deviceId: String,
        targetVersion: String,
        swuURL: URL,
        zckURL: URL,
        bootZckURL: URL,
        artifacts: OtaReleaseArtifacts?,
        config: OtaPollConfig,
        gateway: BridgethingGateway
    ) async {
        guard tryBeginInFlight(deviceId) else { return }
        defer { inFlight.remove(deviceId) }
        let cacheDir = effectiveCacheDir(config: config)
        let (stream, continuation) = AsyncStream.makeStream(of: OtaPhaseSnapshot.self)
        let forwarder = forwardProgress(stream: stream, deviceId: deviceId, kind: .image)
        let swuLocal: URL
        let zckLocal: URL
        let bootZckLocal: URL
        do {
            swuLocal = try await downloadIfNeeded(
                url: swuURL, into: cacheDir, filename: "image-\(config.channel)-\(targetVersion).swu",
                asset: "update.swu", expected: artifacts?.imageSwu, progress: continuation
            )
            zckLocal = try await downloadIfNeeded(
                url: zckURL, into: cacheDir, filename: "image-\(config.channel)-\(targetVersion).zck",
                asset: Self.systemZckAsset, expected: artifacts?.imageZck, progress: continuation
            )
            bootZckLocal = try await downloadIfNeeded(
                url: bootZckURL, into: cacheDir, filename: "image-\(config.channel)-\(targetVersion)-boot.zck",
                asset: Self.bootZckAsset, expected: artifacts?.imageBootZck, progress: continuation
            )
        } catch {
            let reason = "image download failed: \(error.localizedDescription)"
            continuation.yield(.failed(reason: reason))
            continuation.finish()
            _ = await forwarder.value
            eventContinuation.yield(.failed(deviceId: deviceId, kind: .image, reason: reason))
            noteAutoPushResult(deviceId, failed: true)
            return
        }
        await pushUpdate(
            gateway: gateway,
            deviceId: deviceId,
            swuPath: swuLocal,
            zcks: [Self.systemZckAsset: zckLocal, Self.bootZckAsset: bootZckLocal],
            updateUrlBase: config.rootURL.absoluteString,
            progress: continuation
        )
        let terminal = await forwarder.value
        emitTerminal(deviceId: deviceId, kind: .image, version: targetVersion, terminal: terminal)
        if case .failed = terminal { noteAutoPushResult(deviceId, failed: true) } else { noteAutoPushResult(deviceId, failed: false) }
    }

    private func emitTerminal(
        deviceId: String,
        kind: OtaKind,
        version: String,
        terminal: OtaPhaseSnapshot
    ) {
        switch terminal {
        case .completed:
            eventContinuation.yield(.updated(deviceId: deviceId, kind: kind, version: version))
        case let .failed(reason):
            eventContinuation.yield(.failed(deviceId: deviceId, kind: kind, reason: reason))
        case .idle, .downloading, .streaming, .rangePull, .applying, .staged:
            eventContinuation.yield(.updated(deviceId: deviceId, kind: kind, version: version))
        }
    }

    private nonisolated func forwardProgress(
        stream: AsyncStream<OtaPhaseSnapshot>,
        deviceId: String,
        kind: OtaKind
    ) -> Task<OtaPhaseSnapshot, Never> {
        let continuation = eventContinuation
        return Task {
            var last: OtaPhaseSnapshot = .idle
            for await snapshot in stream {
                last = snapshot
                continuation.yield(.progress(deviceId: deviceId, kind: kind, snapshot: snapshot))
            }
            return last
        }
    }

    private func effectiveCacheDir(config: OtaPollConfig) -> URL {
        if let dir = config.cacheDirectory { return dir }
        let base = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first
            ?? URL(fileURLWithPath: NSTemporaryDirectory())
        return base.appendingPathComponent("bridgething-ota", isDirectory: true)
    }

    private func fetchManifest(url: URL) async throws -> OtaDiscoverManifest {
        var req = URLRequest(url: url)
        req.cachePolicy = .reloadIgnoringLocalCacheData
        req.timeoutInterval = 30
        let (data, response) = try await URLSession.shared.data(for: req)
        if let http = response as? HTTPURLResponse, !(200 ..< 300).contains(http.statusCode) {
            throw OtaServiceError.manifestHttpStatus(http.statusCode)
        }
        let decoder = JSONDecoder()
        return try decoder.decode(OtaDiscoverManifest.self, from: data)
    }

    private func downloadIfNeeded(
        url: URL,
        into directory: URL,
        filename: String,
        asset: String,
        expected: OtaArtifactDigest?,
        progress: AsyncStream<OtaPhaseSnapshot>.Continuation?
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

        let tracker = RateTracker()
        progress?.yield(.downloading(asset: asset, received: 0, total: expected?.size ?? 0, ratePerSec: nil))
        let tmp: URL
        let response: URLResponse
        #if canImport(FoundationNetworking)
            (tmp, response) = try await URLSession.shared.download(from: url)
        #else
            let delegate = DownloadProgressDelegate { received, total in
                tracker.record(UInt64(max(received, 0)))
                progress?.yield(.downloading(
                    asset: asset,
                    received: UInt64(max(received, 0)),
                    total: UInt64(max(total, 0)),
                    ratePerSec: tracker.ratePerSec()
                ))
            }
            (tmp, response) = try await URLSession.shared.download(for: URLRequest(url: url), delegate: delegate)
        #endif
        if let http = response as? HTTPURLResponse, !(200 ..< 300).contains(http.statusCode) {
            try? FileManager.default.removeItem(at: tmp)
            throw OtaServiceError.artifactHttpStatus(http.statusCode)
        }
        if let expected {
            let size = (try FileManager.default.attributesOfItem(atPath: tmp.path)[.size] as? NSNumber)?.uint64Value ?? 0
            guard size == expected.size else {
                try? FileManager.default.removeItem(at: tmp)
                throw OtaServiceError.digestMismatch(asset: asset, field: "size")
            }
            let sha = try await hashFile(tmp)
            guard sha == expected.sha256 else {
                try? FileManager.default.removeItem(at: tmp)
                throw OtaServiceError.digestMismatch(asset: asset, field: "sha256")
            }
        }
        if FileManager.default.fileExists(atPath: target.path) {
            try FileManager.default.removeItem(at: target)
        }
        try FileManager.default.moveItem(at: tmp, to: target)
        return target
    }

    // MARK: - inbound range serving

    private func handleRangeRequest(
        gateway: BridgethingGateway,
        handle: OtaAssetRangeHandle,
        req: OtaAssetRange
    ) async {
        guard let zck = localZcks[req.asset] else {
            try? await handle.respondErr(OtaAssetRangeRejected(
                reason: "companion has no cached .zck for asset \(req.asset)"
            ))
            return
        }
        let attrs: [FileAttributeKey: Any]
        do {
            attrs = try FileManager.default.attributesOfItem(atPath: zck.path)
        } catch {
            try? await handle.respondErr(OtaAssetRangeRejected(
                reason: "stat zck failed: \(error.localizedDescription)"
            ))
            return
        }
        guard let totalSize64 = (attrs[.size] as? NSNumber)?.uint64Value, totalSize64 <= UInt64(UInt32.max) else {
            try? await handle.respondErr(OtaAssetRangeRejected(
                reason: "zck size unavailable or > 4 GiB"
            ))
            return
        }
        let totalSize = UInt32(totalSize64)
        for r in req.ranges {
            let endResult = r.start.addingReportingOverflow(r.length)
            if endResult.overflow || endResult.partialValue > totalSize {
                try? await handle.respondErr(OtaAssetRangeRejected(
                    reason: "range \(r.start)+\(r.length) exceeds zck size \(totalSize)"
                ))
                return
            }
        }
        let parts = req.ranges.map { RangePart(start: $0.start, length: $0.length) }
        let streamLen = parts.reduce(UInt32(0)) { $0 + $1.length }

        let fileHandle: FileHandle
        do {
            fileHandle = try FileHandle(forReadingFrom: zck)
        } catch {
            try? await handle.respondErr(OtaAssetRangeRejected(reason: "open zck failed"))
            return
        }
        defer { try? fileHandle.close() }

        let inlineMax: UInt32 = 16 * 1024
        if streamLen <= inlineMax {
            var body = Data(capacity: Int(streamLen))
            for part in parts {
                do {
                    try fileHandle.seek(toOffset: UInt64(part.start))
                    guard let piece = try fileHandle.read(upToCount: Int(part.length)), piece.count == Int(part.length) else {
                        try? await handle.respondErr(OtaAssetRangeRejected(reason: "short read from zck"))
                        return
                    }
                    body.append(piece)
                } catch {
                    try? await handle.respondErr(OtaAssetRangeRejected(reason: "read zck failed"))
                    return
                }
            }
            try? await handle.respond(OtaAssetRangeReply(totalSize: totalSize, parts: parts, body: .inline(body)))
            noteRangeServed(deviceId: handle.deviceId, asset: req.asset, bytes: streamLen)
            return
        }

        do {
            try await handle.respond(OtaAssetRangeReply(
                totalSize: totalSize,
                parts: parts,
                body: .stream(TransferRef(id: handle.requestId, totalSize: streamLen, sha256: nil))
            ))
        } catch {
            return
        }

        let chunkBytes: UInt32 = 64 * 1024
        var streamOffset: UInt32 = 0
        for part in parts {
            do {
                try fileHandle.seek(toOffset: UInt64(part.start))
            } catch {
                return
            }
            var produced: UInt32 = 0
            while produced < part.length {
                let want = Int(min(chunkBytes, part.length - produced))
                let data: Data
                do {
                    data = try fileHandle.read(upToCount: want) ?? Data()
                } catch {
                    return
                }
                if data.isEmpty { return }
                produced += UInt32(data.count)
                do {
                    try await gateway.device(handle.deviceId).transfer.fragment(
                        TransferFragment(transferId: handle.requestId, offset: streamOffset, bytes: data),
                        priority: .background
                    )
                } catch {
                    return
                }
                streamOffset += UInt32(data.count)
            }
        }
        noteRangeServed(deviceId: handle.deviceId, asset: req.asset, bytes: streamLen)
    }

    private func noteRangeServed(deviceId: String, asset: String, bytes: UInt32) {
        guard imageProgressDeviceId == deviceId, let progress = imageProgress else { return }
        let served = (rangeServed[asset] ?? 0) + UInt64(bytes)
        rangeServed[asset] = served
        let tracker = rangeTrackers[asset] ?? RateTracker()
        tracker.record(served)
        rangeTrackers[asset] = tracker
        progress.yield(.rangePull(asset: asset, served: served, ratePerSec: tracker.ratePerSec()))
    }

    // MARK: - push-side driver

    private enum DriveMode {
        case full
        case stage
    }

    private func driveOta(
        gateway: BridgethingGateway,
        deviceId: String,
        kind: OtaKind,
        artifactPath: URL,
        updateUrlBase: String?,
        mode: DriveMode,
        progress: AsyncStream<OtaPhaseSnapshot>.Continuation
    ) async -> (snapshot: OtaPhaseSnapshot, updateId: String) {
        let totalSize: UInt64
        do {
            let attrs = try FileManager.default.attributesOfItem(atPath: artifactPath.path)
            guard let n = (attrs[.size] as? NSNumber)?.uint64Value else {
                return (.failed(reason: "could not stat artifact"), "")
            }
            totalSize = n
        } catch {
            return (.failed(reason: "stat artifact failed: \(error.localizedDescription)"), "")
        }
        guard totalSize <= UInt64(UInt32.max) else {
            return (.failed(reason: "artifact larger than 4 GiB"), "")
        }

        let sha256: String
        do {
            sha256 = try await hashFile(artifactPath)
        } catch {
            return (.failed(reason: "sha256 failed: \(error.localizedDescription)"), "")
        }

        let transferId = UUID()
        let begin = OtaBegin(
            kind: kind,
            updateId: sha256,
            updateUrlBase: updateUrlBase,
            transfer: TransferRef(id: transferId, totalSize: UInt32(totalSize), sha256: sha256)
        )
        let beginResult: RequestResult<OtaBeginAck, OtaBeginRejected>
        do {
            beginResult = try await gateway.system.otaBegin(deviceId: deviceId, begin)
        } catch {
            return (.failed(reason: "OtaBegin send failed: \(error.localizedDescription)"), sha256)
        }
        let resumeFromOffset: UInt32
        switch beginResult {
        case let .ok(ack):
            resumeFromOffset = ack.resumeFromOffset
        case let .domain(rej):
            return (.failed(reason: "daemon rejected OtaBegin: \(rej.reason)"), sha256)
        case let .protocolError(err):
            return (.failed(reason: "OtaBegin protocol error: \(err)"), sha256)
        }

        if kind == .image {
            imageProgress = progress
            imageProgressDeviceId = deviceId
            rangeServed = [:]
            rangeTrackers = [:]
        }
        defer {
            if kind == .image {
                imageProgress = nil
                imageProgressDeviceId = nil
                rangeServed = [:]
                rangeTrackers = [:]
            }
        }

        progress.yield(.streaming(sent: UInt64(resumeFromOffset), total: totalSize, ratePerSec: nil, etaSeconds: nil))

        let terminalTask = awaitTerminal(gateway: gateway, mode: mode, progress: progress)

        let terminal: OtaPhaseSnapshot = await withTaskGroup(of: OtaPhaseSnapshot?.self) { group in
            group.addTask { await terminalTask.value }
            group.addTask {
                do {
                    try await self.streamArtifact(
                        gateway: gateway,
                        deviceId: deviceId,
                        transferId: transferId,
                        artifactPath: artifactPath,
                        startOffset: UInt64(resumeFromOffset),
                        totalSize: totalSize,
                        progress: progress
                    )
                    return nil
                } catch is CancellationError {
                    return nil
                } catch {
                    return .failed(reason: "chunk stream failed: \(error.localizedDescription)")
                }
            }
            var resolved: OtaPhaseSnapshot?
            for await r in group where r != nil {
                resolved = r
                break
            }
            group.cancelAll()
            return resolved ?? .failed(reason: "ota ended without a terminal")
        }
        terminalTask.cancel()
        if case .failed = terminal {
            try? await gateway.device(deviceId).transfer.abandon(
                TransferAbandon(transferId: transferId, reason: "attempt ended")
            )
        }
        return (terminal, sha256)
    }

    private func applyBandaidBatch(
        gateway: BridgethingGateway,
        deviceId: String,
        artifacts: [(kind: OtaKind, path: URL)],
        progress: AsyncStream<OtaPhaseSnapshot>.Continuation
    ) async -> OtaPhaseSnapshot {
        var stagedIds: [String] = []
        for artifact in artifacts {
            let (snapshot, updateId) = await driveOta(
                gateway: gateway,
                deviceId: deviceId,
                kind: artifact.kind,
                artifactPath: artifact.path,
                updateUrlBase: nil,
                mode: .stage,
                progress: progress
            )
            guard case .staged = snapshot else {
                return snapshot
            }
            stagedIds.append(updateId)
        }
        return await commitBandaid(gateway: gateway, deviceId: deviceId, expected: stagedIds, progress: progress)
    }

    private func commitBandaid(
        gateway: BridgethingGateway,
        deviceId: String,
        expected: [String],
        progress: AsyncStream<OtaPhaseSnapshot>.Continuation
    ) async -> OtaPhaseSnapshot {
        let terminalTask = awaitTerminal(gateway: gateway, mode: .full, progress: progress)
        do {
            try await gateway.device(deviceId).system.otaActivate(OtaActivate(expected: expected))
        } catch {
            terminalTask.cancel()
            return .failed(reason: "OtaActivate send failed: \(error.localizedDescription)")
        }
        return await terminalTask.value
    }

    private static let otaIdleDeadline: TimeInterval = 60

    private actor ProgressClock {
        private var last = Date()
        func touch() { last = Date() }
        func idleSeconds() -> TimeInterval { Date().timeIntervalSince(last) }
    }

    private func awaitTerminal(
        gateway: BridgethingGateway,
        mode: DriveMode,
        progress: AsyncStream<OtaPhaseSnapshot>.Continuation
    ) -> Task<OtaPhaseSnapshot, Never> {
        let success: OtaPhaseSnapshot
        switch mode {
        case .full: success = .completed
        case .stage: success = .staged
        }
        let clock = ProgressClock()
        let live = gateway.liveEvents
        return Task {
            await withTaskGroup(of: OtaPhaseSnapshot?.self) { group in
                group.addTask {
                    for await event in live {
                        guard case let .message(_, message) = event,
                              case let .system(outer) = message.data
                        else { continue }
                        switch outer {
                        case let .otaProgress(ev):
                            await clock.touch()
                            progress.yield(.applying(phase: ev.phase, percent: Int(ev.percent)))
                            let done: Bool
                            switch mode {
                            case .full: done = ev.phase == .reboot
                            case .stage: done = ev.phase == .writing && ev.percent >= 100
                            }
                            if done { return success }
                        case let .otaError(ev):
                            return .failed(reason: "[\(ev.code)] \(ev.msg)")
                        default:
                            continue
                        }
                    }
                    return nil
                }
                group.addTask {
                    while !Task.isCancelled {
                        try? await Task.sleep(for: .seconds(15))
                        if await clock.idleSeconds() > Self.otaIdleDeadline {
                            return .failed(reason: "ota stalled: no progress within \(Int(Self.otaIdleDeadline))s")
                        }
                    }
                    return nil
                }
                var result: OtaPhaseSnapshot = .failed(reason: "ota ended without a terminal")
                for await r in group {
                    if let r {
                        result = r
                        break
                    }
                }
                group.cancelAll()
                return result
            }
        }
    }

    private func streamArtifact(
        gateway: BridgethingGateway,
        deviceId: String,
        transferId: UUID,
        artifactPath: URL,
        startOffset: UInt64,
        totalSize: UInt64,
        progress: AsyncStream<OtaPhaseSnapshot>.Continuation?
    ) async throws {
        let fh = try FileHandle(forReadingFrom: artifactPath)
        defer {
            try? fh.close()
            Task { await transferAcks.finish(transferId) }
        }
        let tracker = RateTracker()
        var lastEmit = Date.distantPast
        func emitStreaming(_ sent: UInt64) {
            let sent = min(sent, totalSize)
            tracker.record(sent)
            let now = Date()
            guard now.timeIntervalSince(lastEmit) >= 0.25 || sent >= totalSize else { return }
            lastEmit = now
            let remaining = totalSize > sent ? totalSize - sent : 0
            progress?.yield(.streaming(
                sent: sent, total: totalSize,
                ratePerSec: tracker.ratePerSec(), etaSeconds: tracker.etaSeconds(remaining: remaining)
            ))
        }
        if startOffset > 0 {
            try fh.seek(toOffset: startOffset)
            await transferAcks.note(transferId: transferId, received: UInt32(startOffset))
        }
        var offset = startOffset
        while offset < totalSize {
            try Task.checkCancellation()
            // hold no more than otaWindowBytes unacked so a cancelled attempt leaves nothing in flight.
            while true {
                let acked = UInt64(await transferAcks.receivedBytes(transferId))
                emitStreaming(acked)
                if offset < acked + Self.otaWindowBytes { break }
                if !(await transferAcks.waitForProgress(transferId, beyond: UInt32(acked), timeoutSeconds: Self.otaAckTimeoutSeconds)) {
                    throw TransferStalled()
                }
            }
            let want = Int(min(Self.otaFragmentBytes, totalSize - offset))
            let data = try fh.read(upToCount: want) ?? Data()
            if data.isEmpty {
                throw OtaServiceError.unexpectedEof(at: offset, total: totalSize)
            }
            try await gateway.device(deviceId).transfer.fragment(
                TransferFragment(transferId: transferId, offset: UInt32(offset), bytes: data),
                priority: .background
            )
            offset += UInt64(data.count)
        }
        emitStreaming(totalSize)
    }

    private func hashFile(_ url: URL) async throws -> String {
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
            throw OtaServiceError.cryptoUnavailable
        #endif
    }

}

private enum OtaServiceError: Error, CustomStringConvertible, LocalizedError {
    case unexpectedEof(at: UInt64, total: UInt64)
    case cryptoUnavailable
    case manifestHttpStatus(Int)
    case artifactHttpStatus(Int)
    case digestMismatch(asset: String, field: String)

    var description: String {
        switch self {
        case let .unexpectedEof(at: a, total: t): "EOF at \(a)/\(t) before last chunk"
        case .cryptoUnavailable: "CryptoKit unavailable on this platform"
        case let .manifestHttpStatus(code): "manifest fetch returned HTTP \(code)"
        case let .artifactHttpStatus(code): "artifact fetch returned HTTP \(code)"
        case let .digestMismatch(asset, field): "\(asset) \(field) does not match the manifest; refusing to install"
        }
    }

    var errorDescription: String? { description }
}

#if !canImport(FoundationNetworking)
    private final class DownloadProgressDelegate: NSObject, URLSessionDownloadDelegate, @unchecked Sendable {
        private let onProgress: @Sendable (Int64, Int64) -> Void
        init(onProgress: @escaping @Sendable (Int64, Int64) -> Void) { self.onProgress = onProgress }
        func urlSession(
            _: URLSession,
            downloadTask _: URLSessionDownloadTask,
            didWriteData _: Int64,
            totalBytesWritten: Int64,
            totalBytesExpectedToWrite: Int64
        ) {
            onProgress(totalBytesWritten, totalBytesExpectedToWrite)
        }

        func urlSession(_: URLSession, downloadTask _: URLSessionDownloadTask, didFinishDownloadingTo _: URL) {}
    }
#endif
