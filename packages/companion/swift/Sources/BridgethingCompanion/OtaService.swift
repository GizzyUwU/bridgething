import BridgethingGateway
import BridgethingSchema
import Foundation

#if canImport(CryptoKit)
    import CryptoKit
#endif
#if canImport(FoundationNetworking)
    import FoundationNetworking
#endif

/// Snapshot of the current OTA flow visible to the host app's UI.
public enum OtaPhaseSnapshot: Sendable, Equatable {
    /// No OTA in flight.
    case idle
    /// Companion is streaming `.swu` chunks to the daemon.
    case streaming(percent: Int)
    /// Daemon-side phases (verifying / writing / confirming / reboot).
    case applying(phase: OtaPhase, percent: Int)
    /// A bandaid piece (daemon / builtin-webapp) is validated and staged on
    /// the device but not yet live. The batch goes live on `OtaActivate`.
    case staged
    /// Update finished cleanly. Device has rebooted.
    case completed
    /// Update failed.
    case failed(reason: String)
}

/// Terminal outcome of a third-party webapp install (`OtaKind.installedWebapp`).
public enum WebappInstallResult: Sendable {
    case installed(WebappInfo)
    case failed(reason: String)
}

/// Configuration for the manifest poll loop. The host app supplies one
/// of these to opt the device into auto-updates against
/// `ota.bridgething.com` (or a self-hosted equivalent). When unset the
/// service stays in passive mode (range serving + manual push only).
public struct OtaPollConfig: Sendable, Equatable {
    /// Root URL the manifest and artifacts live under. Default
    /// `https://ota.bridgething.com`. Override only for self-hosting
    /// or local development.
    public var rootURL: URL
    /// Channel the host app's user has selected (`stable` or `dev`).
    /// Cross-channel updates are gated behind a UI prompt because the
    /// zcks won't line up; the poll loop emits `channelMismatch`
    /// instead of auto-pushing when this disagrees with what the
    /// device announces in `BridgeThingMeta.channel`.
    public var channel: String
    /// Seconds between polls. Default 21600 (6 hours). Polling is
    /// best-effort; missed wakeups (background-throttled) just defer.
    public var intervalSeconds: TimeInterval
    /// Where to cache fetched artifacts. Defaults to
    /// `Library/Caches/bridgething-ota` on iOS / macOS. Pass an
    /// override only when the host app wants its own cache lifecycle.
    public var cacheDirectory: URL?
    /// When true, a detected version delta auto-pushes. When false the
    /// poll loop only emits `updateAvailable` and the host app drives
    /// the push manually via `pushDaemon` / `pushUpdate`. Default true.
    public var autoPush: Bool

    public init(
        rootURL: URL = URL(string: "https://ota.bridgething.com")!,
        channel: String,
        intervalSeconds: TimeInterval = 21600,
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

/// High-level event from the manifest poll loop. The host app drives
/// UI off these (channel-switch prompts, "downloading update" toast,
/// progress bar). In-flight per-chunk progress comes through as
/// `progress(...)` carrying an `OtaPhaseSnapshot`.
public enum OtaPollEvent: Sendable, Equatable {
    /// Manifest fetch + parse succeeded; carries the manifest's own
    /// `updated_at` for staleness UIs.
    case manifestPolled(updatedAt: String)
    /// Manifest fetch or parse failed. The next interval tick retries.
    case manifestPollFailed(reason: String)
    /// The device's announced `BridgeThingMeta.channel` does not match
    /// the host app's configured channel. Auto-push is suppressed; the
    /// host should prompt the user to reflash if they want to switch.
    case channelMismatch(deviceId: String, deviceChannel: String, configuredChannel: String)
    /// New version detected. Emitted whether or not `autoPush` is on.
    case updateAvailable(deviceId: String, kind: OtaKind, fromVersion: String, toVersion: String)
    /// Per-chunk / per-phase progress for an in-flight update.
    case progress(deviceId: String, kind: OtaKind, snapshot: OtaPhaseSnapshot)
    /// Update finished. For daemon kind, the daemon process restarted
    /// and the gateway link briefly drops + reconnects; for image kind,
    /// the device power-cycled.
    case updated(deviceId: String, kind: OtaKind, version: String)
    /// Update failed (download, push, or daemon-side).
    case failed(deviceId: String, kind: OtaKind, reason: String)
}

/// OTA service for the bridgething companion. Three jobs in one actor:
///
/// 1. Serve inbound `OtaAssetRange` requests from a configured local
///    `.zck` (the daemon's range proxy reads delta bytes through this
///    when applying an image OTA).
/// 2. Drive a manual `pushDaemon` or `pushUpdate` against a target
///    device when the host app supplies a local artifact path.
/// 3. When `setPollConfig(...)` is provided, periodically fetch the
///    discover manifest at `<rootURL>/manifest.json`, compare to each
///    connected device's announced `BridgeThingMeta`, and auto-push
///    daemon + image deltas. Cross-channel deltas surface as
///    `channelMismatch` instead of pushing.
///
/// The host app subscribes to `events` to drive its UI; in-flight
/// progress comes through as `progress(...)` carrying `OtaPhaseSnapshot`.
public actor OtaService {
    /// Where the companion reads byte ranges from when the daemon's
    /// range proxy requests them. Range requests arriving while this
    /// is nil are rejected with `OtaAssetRangeRejected`.
    private var localZck: URL?
    private var rangeServerTask: Task<Void, Never>?
    private var metaTask: Task<Void, Never>?
    private var pollTask: Task<Void, Never>?

    private var attachedGateway: BridgethingGateway?
    private var pollConfig: OtaPollConfig?
    private var deviceMeta: [String: BridgeThingMeta] = [:]
    private var inFlight: Set<String> = []

    private let eventContinuation: AsyncStream<OtaPollEvent>.Continuation

    /// High-level poll-loop events. The host app drives UI from this
    /// stream. Stays open across `start` / `stop` cycles; cancel by
    /// dropping the consumer.
    public nonisolated let events: AsyncStream<OtaPollEvent>

    public init() {
        let (stream, continuation) = AsyncStream.makeStream(of: OtaPollEvent.self)
        events = stream
        eventContinuation = continuation
    }

    /// Start serving inbound `OtaAssetRange` requests and tracking
    /// per-device `BridgeThingMeta`. Safe to call again after `stop()`.
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
                guard case let .message(deviceId, msg) = event,
                      case let .version(meta) = msg.data
                else { continue }
                guard let self else { return }
                await recordMeta(deviceId: deviceId, meta: meta)
            }
        }
    }

    public func stop() async {
        rangeServerTask?.cancel()
        rangeServerTask = nil
        metaTask?.cancel()
        metaTask = nil
        pollTask?.cancel()
        pollTask = nil
        attachedGateway = nil
        deviceMeta.removeAll()
    }

    /// Bind the local `.zck` file the companion reads byte ranges from.
    /// Pass nil to clear (subsequent range requests get rejected).
    public func setLocalZck(_ url: URL?) {
        localZck = url
    }

    public func currentLocalZck() -> URL? { localZck }

    /// Drive an image-kind OTA from a local `.swu` (and matching `.zck`
    /// for delta fetch). Yields `OtaPhaseSnapshot` updates over the
    /// supplied `progress` continuation; finishes the continuation on
    /// terminal state. `updateUrlBase` is recorded on
    /// `OtaBegin.update_url_base` for future cache-miss recovery flows.
    public func pushUpdate(
        gateway: BridgethingGateway,
        deviceId: String,
        swuPath: URL,
        zckPath: URL,
        updateUrlBase: String? = nil,
        progress: AsyncStream<OtaPhaseSnapshot>.Continuation
    ) async {
        setLocalZck(zckPath)
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

    /// Push a new daemon binary to the bandaid. Stages it then activates the
    /// (single-piece) batch, which atomically swaps `.current` and restarts
    /// bridgething.service once. No range proxy traffic for this kind.
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

    /// Push a builtin-webapp bundle (hub or stock) to the bandaid. Same
    /// stage-then-activate path as the daemon; the bundle's manifest id must
    /// be a reserved builtin or the daemon rejects the stage.
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

    /// Push a coupled set of bandaid pieces (daemon + hub + stock, in any
    /// combination) as one batch: every piece stages, then a single
    /// `OtaActivate` swaps them all live with one restart.
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

    /// Install a third-party webapp bundle into the device's writable
    /// registry via `OtaKind.installedWebapp`. Reuses the OTA chunk pump;
    /// no staging, no activate, no restart. The terminal is the daemon's
    /// `WebappInstalled` event (success, carrying the `WebappInfo`) or an
    /// `OtaError` (failure).
    public func installWebapp(
        gateway: BridgethingGateway,
        deviceId: String,
        bundlePath: URL
    ) async -> WebappInstallResult {
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

        // subscribe before streaming so the terminal event cannot be missed.
        let terminalTask = Task<WebappInstallResult, Never> {
            await withTaskGroup(of: WebappInstallResult.self) { group in
                group.addTask {
                    for await pair in gateway.webapp.webappInstalled where pair.deviceId == deviceId {
                        return .installed(pair.msg)
                    }
                    return .failed(reason: "installed stream ended")
                }
                group.addTask {
                    for await pair in gateway.system.otaError where pair.deviceId == deviceId {
                        return .failed(reason: "[\(pair.msg.code)] \(pair.msg.msg)")
                    }
                    return .failed(reason: "error stream ended")
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

        let begin = OtaBegin(
            kind: .installedWebapp,
            updateId: sha256,
            updateUrlBase: nil,
            expectedSha256: sha256,
            expectedSize: UInt32(totalSize)
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
                updateId: sha256,
                artifactPath: bundlePath,
                startOffset: UInt64(resumeFromOffset),
                totalSize: totalSize
            )
        } catch {
            terminalTask.cancel()
            return .failed(reason: "chunk stream failed: \(error.localizedDescription)")
        }

        return await terminalTask.value
    }

    // MARK: - manifest poll loop

    /// Returns the most recent `BridgeThingMeta` the daemon announced
    /// for `deviceId`, or nil if none has been seen yet.
    public func meta(deviceId: String) -> BridgeThingMeta? {
        deviceMeta[deviceId]
    }

    /// Set or replace the manifest poll configuration. Pass nil to
    /// disable polling (range serving + manual push still work). The
    /// new config takes effect immediately: any in-flight poll task is
    /// cancelled and a fresh one starts.
    public func setPollConfig(_ config: OtaPollConfig?) {
        pollConfig = config
        pollTask?.cancel()
        pollTask = nil
        guard let config else { return }
        pollTask = Task { [weak self] in
            await self?.runPollLoop(config: config)
        }
    }

    /// Run one poll iteration immediately, regardless of where the
    /// interval timer is. Useful when the host app foregrounds and
    /// wants a fresh check.
    public func pollNow() async {
        guard let config = pollConfig, let gateway = attachedGateway else { return }
        await poll(config: config, gateway: gateway)
    }

    /// One-shot "check now": polls the channel and emits `updateAvailable` /
    /// `channelMismatch` for connected devices without persisting config or
    /// auto-pushing. Independent of the background poll loop.
    public func checkNow(channel: String, rootURL: URL) async {
        guard let gateway = attachedGateway else { return }
        let transient = OtaPollConfig(rootURL: rootURL, channel: channel, autoPush: false)
        await poll(config: transient, gateway: gateway)
    }

    /// Full discover manifest for the version picker.
    public func discoverManifest(rootURL: URL) async throws -> OtaDiscoverManifest {
        try await fetchManifest(url: rootURL.appendingPathComponent("manifest.json"))
    }

    /// Manual install of a specific composite version. Pushes the daemon delta
    /// first (the image check rides the next poll once the daemon restarts),
    /// then the image delta. Reuses the auto-push engine; ignores `autoPush`.
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
        if meta.appVersion != composite.daemon {
            await runDaemonAuto(
                deviceId: deviceId, targetVersion: composite.daemon,
                binaryURL: urls.daemonBinary, config: config, gateway: gateway
            )
            return
        }
        if meta.imageVersion != composite.image {
            await runImageAuto(
                deviceId: deviceId, targetVersion: composite.image,
                swuURL: urls.imageSwu, zckURL: urls.imageZck, config: config, gateway: gateway
            )
        }
    }

    private func runPollLoop(config: OtaPollConfig) async {
        // First poll fires immediately so a freshly-launched app
        // checks before the interval clock first ticks.
        while !Task.isCancelled {
            if let gateway = attachedGateway {
                await poll(config: config, gateway: gateway)
            }
            let nanos = UInt64(max(config.intervalSeconds, 60) * 1_000_000_000)
            try? await Task.sleep(nanoseconds: nanos)
        }
    }

    private func recordMeta(deviceId: String, meta: BridgeThingMeta) {
        deviceMeta[deviceId] = meta
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
        if let release = manifest.releases[channel.latest] {
            if release.yanked != nil || release.deprecated { return }
        }

        // Snapshot device list so per-device downloads don't keep the actor blocked; reentrant polls
        // observe the inFlight set and skip.
        let snapshot = deviceMeta
        for (deviceId, meta) in snapshot {
            await reconcileDevice(
                deviceId: deviceId,
                meta: meta,
                latest: composite,
                config: config,
                gateway: gateway
            )
        }
    }

    private func reconcileDevice(
        deviceId: String,
        meta: BridgeThingMeta,
        latest: OtaCompositeVersion,
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

        if meta.appVersion != latest.daemon {
            eventContinuation.yield(.updateAvailable(
                deviceId: deviceId,
                kind: .daemon,
                fromVersion: meta.appVersion,
                toVersion: latest.daemon
            ))
            if config.autoPush {
                await runDaemonAuto(
                    deviceId: deviceId,
                    targetVersion: latest.daemon,
                    binaryURL: urls.daemonBinary,
                    config: config,
                    gateway: gateway
                )
            }
            // Daemon push restarts the gateway link; the image check waits for the next poll cycle.
            return
        }

        if meta.imageVersion != latest.image {
            eventContinuation.yield(.updateAvailable(
                deviceId: deviceId,
                kind: .image,
                fromVersion: meta.imageVersion,
                toVersion: latest.image
            ))
            if config.autoPush {
                await runImageAuto(
                    deviceId: deviceId,
                    targetVersion: latest.image,
                    swuURL: urls.imageSwu,
                    zckURL: urls.imageZck,
                    config: config,
                    gateway: gateway
                )
            }
        }
    }

    private func runDaemonAuto(
        deviceId: String,
        targetVersion: String,
        binaryURL: URL,
        config: OtaPollConfig,
        gateway: BridgethingGateway
    ) async {
        inFlight.insert(deviceId)
        defer { inFlight.remove(deviceId) }
        let cacheDir = effectiveCacheDir(config: config)
        let cached: URL
        do {
            cached = try await downloadIfNeeded(
                url: binaryURL,
                into: cacheDir,
                filename: "daemon-\(config.channel)-\(targetVersion)"
            )
        } catch {
            eventContinuation.yield(.failed(
                deviceId: deviceId,
                kind: .daemon,
                reason: "daemon download failed: \(error.localizedDescription)"
            ))
            return
        }
        let (stream, continuation) = AsyncStream.makeStream(of: OtaPhaseSnapshot.self)
        let forwarder = forwardProgress(stream: stream, deviceId: deviceId, kind: .daemon)
        await pushDaemon(
            gateway: gateway,
            deviceId: deviceId,
            binaryPath: cached,
            progress: continuation
        )
        let terminal = await forwarder.value
        emitTerminal(deviceId: deviceId, kind: .daemon, version: targetVersion, terminal: terminal)
    }

    private func runImageAuto(
        deviceId: String,
        targetVersion: String,
        swuURL: URL,
        zckURL: URL,
        config: OtaPollConfig,
        gateway: BridgethingGateway
    ) async {
        inFlight.insert(deviceId)
        defer { inFlight.remove(deviceId) }
        let cacheDir = effectiveCacheDir(config: config)
        let swuLocal: URL
        let zckLocal: URL
        do {
            swuLocal = try await downloadIfNeeded(
                url: swuURL,
                into: cacheDir,
                filename: "image-\(config.channel)-\(targetVersion).swu"
            )
            zckLocal = try await downloadIfNeeded(
                url: zckURL,
                into: cacheDir,
                filename: "image-\(config.channel)-\(targetVersion).zck"
            )
        } catch {
            eventContinuation.yield(.failed(
                deviceId: deviceId,
                kind: .image,
                reason: "image download failed: \(error.localizedDescription)"
            ))
            return
        }
        let (stream, continuation) = AsyncStream.makeStream(of: OtaPhaseSnapshot.self)
        let forwarder = forwardProgress(stream: stream, deviceId: deviceId, kind: .image)
        await pushUpdate(
            gateway: gateway,
            deviceId: deviceId,
            swuPath: swuLocal,
            zckPath: zckLocal,
            updateUrlBase: config.rootURL.absoluteString,
            progress: continuation
        )
        let terminal = await forwarder.value
        emitTerminal(deviceId: deviceId, kind: .image, version: targetVersion, terminal: terminal)
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
        case .idle, .streaming, .applying, .staged:
            // Stream ended without a terminal snapshot; treat as success since the auto path
            // only finishes on a committed batch (.completed) or an explicit failure.
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

    private func downloadIfNeeded(url: URL, into directory: URL, filename: String) async throws -> URL {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let target = directory.appendingPathComponent(filename)
        let attrs = try? FileManager.default.attributesOfItem(atPath: target.path)
        if let size = attrs?[.size] as? NSNumber, size.intValue > 0 {
            return target
        }
        let (tmp, response) = try await URLSession.shared.download(from: url)
        if let http = response as? HTTPURLResponse, !(200 ..< 300).contains(http.statusCode) {
            try? FileManager.default.removeItem(at: tmp)
            throw OtaServiceError.artifactHttpStatus(http.statusCode)
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
        guard let zck = localZck else {
            try? await handle.respondErr(OtaAssetRangeRejected(
                reason: "companion has no .zck cached"
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
        do {
            try await handle.respond(OtaAssetRangeReply(totalSize: totalSize, parts: parts))
        } catch {
            return
        }

        let fileHandle: FileHandle
        do {
            fileHandle = try FileHandle(forReadingFrom: zck)
        } catch {
            return
        }
        defer { try? fileHandle.close() }

        let chunkBytes: UInt32 = 64 * 1024
        for (idx, part) in parts.enumerated() {
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
                let absoluteOffset = part.start + produced
                produced += UInt32(data.count)
                let last = idx + 1 == parts.count && produced == part.length
                let chunk = OtaAssetRangeChunk(
                    requestId: handle.requestId,
                    partIndex: UInt32(idx),
                    offset: absoluteOffset,
                    bytes: data,
                    last: last
                )
                do {
                    try await gateway.device(handle.deviceId).system
                        .otaAssetRangeChunk(chunk, priority: .bulk)
                } catch {
                    return
                }
            }
        }
    }

    // MARK: - push-side driver

    private enum DriveMode {
        /// Image: stream, then await `Reboot` (the device power-cycles).
        case full
        /// Bandaid (daemon / builtin-webapp): stream, then await `Writing`/100,
        /// which means the piece is staged but not yet live. The batch goes
        /// live on a later `OtaActivate`.
        case stage
    }

    /// Stream one artifact and await its terminal. Returns the terminal
    /// snapshot plus the artifact's sha256 (the `update_id`), which the
    /// caller passes to `OtaActivate.expected` for a bandaid batch.
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

        let begin = OtaBegin(
            kind: kind,
            updateId: sha256,
            updateUrlBase: updateUrlBase,
            expectedSha256: sha256,
            expectedSize: UInt32(totalSize)
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

        progress.yield(.streaming(percent: percent(UInt64(resumeFromOffset), totalSize)))

        // subscribe before streaming so the terminal event cannot be missed.
        let terminalTask = awaitTerminal(gateway: gateway, mode: mode, progress: progress)

        do {
            try await streamArtifact(
                gateway: gateway,
                deviceId: deviceId,
                updateId: sha256,
                artifactPath: artifactPath,
                startOffset: UInt64(resumeFromOffset),
                totalSize: totalSize
            )
        } catch {
            terminalTask.cancel()
            return (.failed(reason: "chunk stream failed: \(error.localizedDescription)"), sha256)
        }

        return (await terminalTask.value, sha256)
    }

    /// Stage each artifact on the bandaid, then activate the whole batch with
    /// a single `OtaActivate` (one service restart). Returns the terminal
    /// snapshot; does not finish `progress` (the public wrappers do).
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

    /// Send `OtaActivate` and await the single `Reboot`. Subscribes before
    /// sending so the terminal cannot be missed.
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

    /// Watch the progress + error streams and resolve to a terminal: the
    /// mode's success snapshot when the phase predicate fires, or `.failed`
    /// on an `OtaError`. The error arm parks (rather than returning) if its
    /// stream ends without an error, so the progress arm wins the race.
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
        return Task {
            await withTaskGroup(of: OtaPhaseSnapshot.self) { group in
                group.addTask {
                    for await ev in gateway.system.otaProgress {
                        progress.yield(.applying(phase: ev.msg.phase, percent: Int(ev.msg.percent)))
                        let done: Bool
                        switch mode {
                        case .full: done = ev.msg.phase == .reboot
                        case .stage: done = ev.msg.phase == .writing && ev.msg.percent >= 100
                        }
                        if done { break }
                    }
                    return success
                }
                group.addTask {
                    for await ev in gateway.system.otaError {
                        return .failed(reason: "[\(ev.msg.code)] \(ev.msg.msg)")
                    }
                    try? await Task.sleep(for: .seconds(3600))
                    return success
                }
                let result = await group.next() ?? success
                group.cancelAll()
                return result
            }
        }
    }

    private func streamArtifact(
        gateway: BridgethingGateway,
        deviceId: String,
        updateId: String,
        artifactPath: URL,
        startOffset: UInt64,
        totalSize: UInt64
    ) async throws {
        let chunkSize = 64 * 1024
        let fh = try FileHandle(forReadingFrom: artifactPath)
        defer { try? fh.close() }
        if startOffset > 0 {
            try fh.seek(toOffset: startOffset)
        }
        var offset = startOffset
        while offset < totalSize {
            let want = Int(min(UInt64(chunkSize), totalSize - offset))
            let data = try fh.read(upToCount: want) ?? Data()
            if data.isEmpty {
                throw OtaServiceError.unexpectedEof(at: offset, total: totalSize)
            }
            let last = offset + UInt64(data.count) == totalSize
            let chunk = OtaChunk(
                updateId: updateId,
                offset: UInt32(offset),
                bytes: data,
                last: last
            )
            try await gateway.device(deviceId).system.otaChunk(chunk, priority: .bulk)
            offset += UInt64(data.count)
        }
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

    private func percent(_ n: UInt64, _ d: UInt64) -> Int {
        if d == 0 { return 100 }
        let p = n.multipliedReportingOverflow(by: 100).0 / d
        return Int(min(UInt64(100), p))
    }
}

private enum OtaServiceError: Error, CustomStringConvertible, LocalizedError {
    case unexpectedEof(at: UInt64, total: UInt64)
    case cryptoUnavailable
    case manifestHttpStatus(Int)
    case artifactHttpStatus(Int)

    var description: String {
        switch self {
        case let .unexpectedEof(at: a, total: t): "EOF at \(a)/\(t) before last chunk"
        case .cryptoUnavailable: "CryptoKit unavailable on this platform"
        case let .manifestHttpStatus(code): "manifest fetch returned HTTP \(code)"
        case let .artifactHttpStatus(code): "artifact fetch returned HTTP \(code)"
        }
    }

    var errorDescription: String? { description }
}
