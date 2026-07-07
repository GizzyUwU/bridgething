package com.bridgething.companion

import com.bridgething.gateway.BridgethingGateway
import com.bridgething.gateway.GatewayEvent
import com.bridgething.gateway.OtaAssetRangeHandle
import com.bridgething.gateway.RequestResult
import com.bridgething.gateway.device
import com.bridgething.gateway.system
import com.bridgething.gateway.transfer
import com.bridgething.gateway.webapp
import com.bridgething.schema.BridgeThingMeta
import com.bridgething.schema.BridgeToGatewayMsgData
import com.bridgething.schema.OtaAssetRange
import com.bridgething.schema.OtaAssetRangeRejected
import com.bridgething.schema.OtaAssetRangeReply
import com.bridgething.schema.OtaActivate
import com.bridgething.schema.OtaBegin
import com.bridgething.schema.OtaKind
import com.bridgething.schema.OtaPhase
import com.bridgething.schema.Priority
import com.bridgething.schema.RangePart
import com.bridgething.schema.TransferBody
import com.bridgething.schema.TransferAbandon
import com.bridgething.schema.TransferFragment
import com.bridgething.schema.TransferRef
import com.bridgething.schema.WebappInfo
import java.io.File
import java.util.UUID
import java.io.IOException
import java.io.RandomAccessFile
import java.security.MessageDigest
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.Json
import okhttp3.OkHttpClient
import okhttp3.Request

/**
 * Snapshot of the current OTA flow visible to the host app's UI.
 */
public sealed class OtaPhaseSnapshot {
    public object Idle : OtaPhaseSnapshot()
    public data class Streaming(val percent: Int) : OtaPhaseSnapshot()
    public data class Applying(val phase: OtaPhase, val percent: Int) : OtaPhaseSnapshot()

    /**
     * A bandaid piece (daemon / builtin-webapp) is validated and staged on
     * the device but not yet live. The batch goes live on `OtaActivate`.
     */
    public object Staged : OtaPhaseSnapshot()
    public object Completed : OtaPhaseSnapshot()
    public data class Failed(val reason: String) : OtaPhaseSnapshot()
}

/** Terminal outcome of a third-party webapp install (`OtaKind.InstalledWebapp`). */
public sealed class WebappInstallResult {
    public data class Installed(val info: WebappInfo) : WebappInstallResult()
    public data class Failed(val reason: String) : WebappInstallResult()
}

/**
 * Configuration for the manifest poll loop.
 *
 * Set to opt the device into auto-updates against `ota.bridgething.com`
 * (or a self-hosted equivalent). When unset the service stays in passive
 * mode (range serving + manual push only).
 */
public data class OtaPollConfig(
    val rootUrl: String = "https://ota.bridgething.com",
    val channel: String,
    val intervalSeconds: Long = 6 * 60 * 60L,
    val cacheDirectory: File? = null,
    val autoPush: Boolean = true,
)

/**
 * High-level event from the manifest poll loop. The host app drives UI
 * off these (channel-switch prompts, "downloading update" toast,
 * progress bar). In-flight per-chunk progress comes through as
 * [Progress] carrying an [OtaPhaseSnapshot].
 */
public sealed class OtaPollEvent {
    public data class ManifestPolled(val updatedAt: String) : OtaPollEvent()
    public data class ManifestPollFailed(val reason: String) : OtaPollEvent()
    public data class ChannelMismatch(
        val deviceId: String,
        val deviceChannel: String,
        val configuredChannel: String,
    ) : OtaPollEvent()
    public data class UpdateAvailable(
        val deviceId: String,
        val kind: OtaKind,
        val fromVersion: String,
        val toVersion: String,
    ) : OtaPollEvent()
    public data class Progress(
        val deviceId: String,
        val kind: OtaKind,
        val snapshot: OtaPhaseSnapshot,
    ) : OtaPollEvent()
    public data class Updated(
        val deviceId: String,
        val kind: OtaKind,
        val version: String,
    ) : OtaPollEvent()
    public data class Failed(
        val deviceId: String,
        val kind: OtaKind,
        val reason: String,
    ) : OtaPollEvent()
}

/**
 * OTA service for the bridgething companion.
 *
 * Three jobs in one class:
 *
 * 1. Serve inbound `OtaAssetRange` requests from a configured local
 *    `.zck` (the daemon's range proxy reads delta bytes through this
 *    when applying an image OTA).
 * 2. Drive a manual `pushDaemon` or `pushUpdate` against a target
 *    device when the host app supplies a local artifact path.
 * 3. When [setPollConfig] is provided, periodically fetch the discover
 *    manifest at `<rootUrl>/manifest.json`, compare to each connected
 *    device's announced [BridgeThingMeta], and auto-push daemon + image
 *    deltas. Cross-channel deltas surface as `ChannelMismatch` instead
 *    of pushing.
 *
 * The host app subscribes to [events] to drive its UI; in-flight
 * progress comes through as [OtaPollEvent.Progress] carrying an
 * [OtaPhaseSnapshot].
 */
public class OtaService(
    private val httpClient: OkHttpClient = OkHttpClient(),
    private val json: Json = defaultJson,
) : WebappInstaller {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    internal val transferAcks = TransferAckWindow()
    private val mutex = Mutex()
    private val deviceMetaMutex = Mutex()

    private var localZck: File? = null
    private var rangeServerJob: Job? = null
    private var metaJob: Job? = null
    private var nicknameJob: Job? = null
    private var pollJob: Job? = null

    private var attachedGateway: BridgethingGateway? = null
    private var pollConfig: OtaPollConfig? = null
    private val deviceMeta = mutableMapOf<String, BridgeThingMeta>()
    private val inFlight = mutableSetOf<String>()
    private val autoPushNextAt = mutableMapOf<String, Long>()
    private val autoPushFailures = mutableMapOf<String, Int>()
    private var pollWake: CompletableDeferred<Unit>? = null

    private val eventsFlow = MutableSharedFlow<OtaPollEvent>(
        extraBufferCapacity = 64,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )

    public val events: Flow<OtaPollEvent> = eventsFlow.asSharedFlow()

    private val metaChangedFlow = MutableSharedFlow<Pair<String, BridgeThingMeta>>(
        replay = 16,
        extraBufferCapacity = 256,
        onBufferOverflow = BufferOverflow.SUSPEND,
    )

    public val metaChanged: Flow<Pair<String, BridgeThingMeta>> = metaChangedFlow.asSharedFlow()

    public suspend fun start(gateway: BridgethingGateway) {
        mutex.withLock {
            attachedGateway = gateway
            rangeServerJob?.cancel()
            metaJob?.cancel()
            nicknameJob?.cancel()
            rangeServerJob = scope.launch {
                gateway.system.otaAssetRangeRequests.collect { (handle, req) ->
                    launch { handleRangeRequest(gateway, handle, req) }
                }
            }
            metaJob = scope.launch {
                gateway.events.collect { event ->
                    when (event) {
                        is GatewayEvent.Connected -> wakePoll()
                        is GatewayEvent.Message -> {
                            val data = event.message.data
                            if (data is BridgeToGatewayMsgData.Version) {
                                recordMeta(event.deviceId, data.data)
                            }
                        }
                        else -> {}
                    }
                }
            }
            nicknameJob = scope.launch {
                gateway.system.deviceNicknameChanged.collect { (deviceId, reply) ->
                    recordNickname(deviceId, reply.nickname)
                }
            }
        }
    }

    public suspend fun stop() {
        mutex.withLock {
            rangeServerJob?.cancel(); rangeServerJob = null
            metaJob?.cancel(); metaJob = null
            nicknameJob?.cancel(); nicknameJob = null
            pollJob?.cancel(); pollJob = null
            attachedGateway = null
        }
        deviceMetaMutex.withLock { deviceMeta.clear() }
    }

    public fun setLocalZck(file: File?) {
        localZck = file
    }

    public fun currentLocalZck(): File? = localZck

    public suspend fun pushUpdate(
        gateway: BridgethingGateway,
        deviceId: String,
        swuPath: File,
        zckPath: File,
        updateUrlBase: String? = null,
    ): Flow<OtaPhaseSnapshot> {
        setLocalZck(zckPath)
        return runOtaFlow { collector ->
            val (terminal, _) = driveOta(
                gateway = gateway,
                deviceId = deviceId,
                kind = OtaKind.Image,
                artifactPath = swuPath,
                updateUrlBase = updateUrlBase,
                mode = DriveMode.Full,
                emit = collector,
            )
            collector(terminal)
        }
    }

    public suspend fun pushDaemon(
        gateway: BridgethingGateway,
        deviceId: String,
        binaryPath: File,
    ): Flow<OtaPhaseSnapshot> {
        return runOtaFlow { collector ->
            val terminal = applyBandaidBatch(gateway, deviceId, listOf(OtaKind.Daemon to binaryPath), collector)
            collector(terminal)
        }
    }

    public suspend fun pushBuiltinWebapp(
        gateway: BridgethingGateway,
        deviceId: String,
        bundlePath: File,
    ): Flow<OtaPhaseSnapshot> {
        return runOtaFlow { collector ->
            val terminal = applyBandaidBatch(gateway, deviceId, listOf(OtaKind.BuiltinWebapp to bundlePath), collector)
            collector(terminal)
        }
    }

    public suspend fun pushBandaidBatch(
        gateway: BridgethingGateway,
        deviceId: String,
        artifacts: List<Pair<OtaKind, File>>,
    ): Flow<OtaPhaseSnapshot> {
        return runOtaFlow { collector ->
            val terminal = applyBandaidBatch(gateway, deviceId, artifacts, collector)
            collector(terminal)
        }
    }

    override suspend fun installWebapp(
        gateway: BridgethingGateway,
        deviceId: String,
        bundlePath: File,
    ): WebappInstallResult {
        val totalSize = try {
            bundlePath.length()
        } catch (e: Throwable) {
            return WebappInstallResult.Failed("stat bundle failed: ${e.message ?: e.toString()}")
        }
        if (totalSize <= 0L) return WebappInstallResult.Failed("could not stat bundle")
        if (totalSize > UInt.MAX_VALUE.toLong()) return WebappInstallResult.Failed("bundle larger than 4 GiB")
        val sha = try {
            hashFile(bundlePath)
        } catch (e: Throwable) {
            return WebappInstallResult.Failed("sha256 failed: ${e.message ?: e.toString()}")
        }

        val terminal = CompletableDeferred<WebappInstallResult>()
        val installedJob = scope.launch {
            gateway.webapp.webappInstalled.collect { (devId, info) ->
                if (devId == deviceId && terminal.isActive) terminal.complete(WebappInstallResult.Installed(info))
            }
        }
        val errorJob = scope.launch {
            gateway.device(deviceId).system.otaError.collect { e ->
                if (terminal.isActive) terminal.complete(WebappInstallResult.Failed("[${e.code}] ${e.msg}"))
            }
        }
        val timeoutJob = scope.launch {
            delay(60_000)
            if (terminal.isActive) terminal.complete(WebappInstallResult.Failed("install timed out"))
        }
        fun stopJobs() {
            installedJob.cancel(); errorJob.cancel(); timeoutJob.cancel()
        }

        val transferId = UUID.randomUUID()
        val begin = OtaBegin(
            kind = OtaKind.InstalledWebapp,
            updateId = sha,
            updateUrlBase = null,
            transfer = TransferRef(id = transferId, totalSize = totalSize.toUInt(), sha256 = sha),
        )
        val resumeFrom: UInt = try {
            when (val res = gateway.device(deviceId).system.otaBegin(begin)) {
                is RequestResult.Ok -> res.response.resumeFromOffset
                is RequestResult.DomainErr -> {
                    stopJobs()
                    return WebappInstallResult.Failed("daemon rejected install: ${res.error.reason}")
                }
                is RequestResult.ProtocolErr -> {
                    stopJobs()
                    return WebappInstallResult.Failed("OtaBegin protocol error: ${res.error}")
                }
            }
        } catch (e: Throwable) {
            stopJobs()
            return WebappInstallResult.Failed("OtaBegin send failed: ${e.message ?: e.toString()}")
        }

        try {
            streamArtifact(
                gateway = gateway,
                deviceId = deviceId,
                transferId = transferId,
                artifactPath = bundlePath,
                startOffset = resumeFrom.toLong(),
                totalSize = totalSize,
            )
        } catch (e: Throwable) {
            stopJobs()
            return WebappInstallResult.Failed("chunk stream failed: ${e.message ?: e.toString()}")
        }

        return try {
            terminal.await()
        } finally {
            installedJob.cancelAndJoin()
            errorJob.cancelAndJoin()
            timeoutJob.cancelAndJoin()
        }
    }

    public suspend fun meta(deviceId: String): BridgeThingMeta? = deviceMetaMutex.withLock {
        deviceMeta[deviceId]
    }

    public suspend fun setPollConfig(config: OtaPollConfig?) {
        mutex.withLock {
            pollConfig = config
            pollJob?.cancel()
            pollJob = null
            if (config != null) {
                pollJob = scope.launch { runPollLoop(config) }
            }
        }
    }

    public suspend fun pollNow() {
        val (cfg, gw) = mutex.withLock { pollConfig to attachedGateway }
        if (cfg != null && gw != null) poll(cfg, gw)
    }

    public suspend fun checkNow(channel: String, rootUrl: String) {
        val gw = mutex.withLock { attachedGateway } ?: return
        poll(OtaPollConfig(rootUrl = rootUrl, channel = channel, autoPush = false), gw)
    }

    public suspend fun discoverManifest(rootUrl: String): OtaDiscoverManifest =
        fetchManifest("${rootUrl.trimEnd('/')}/manifest.json")

    public suspend fun applyVersion(deviceId: String, channel: String, version: String, rootUrl: String) {
        val gateway = mutex.withLock { attachedGateway } ?: run {
            eventsFlow.emit(OtaPollEvent.Failed(deviceId, OtaKind.Image, "gateway not attached"))
            return
        }
        val composite = OtaCompositeVersion.parse(version) ?: run {
            eventsFlow.emit(OtaPollEvent.Failed(deviceId, OtaKind.Image, "'$version' is not a composite version"))
            return
        }
        val meta = deviceMetaMutex.withLock { deviceMeta[deviceId] } ?: run {
            eventsFlow.emit(OtaPollEvent.Failed(deviceId, OtaKind.Image, "device meta not yet known"))
            return
        }
        if (mutex.withLock { deviceId in inFlight }) return
        val config = OtaPollConfig(rootUrl = rootUrl, channel = channel)
        val urls = OtaArtifactUrls.build(
            rootUrl = rootUrl,
            channel = channel,
            daemonVersion = composite.daemon,
            imageVersion = composite.image,
            imageVariant = meta.imageVariant,
        )
        if (meta.appVersion != composite.daemon) {
            runBandaidBatchAuto(
                deviceId,
                listOf(BandaidPiece(OtaKind.Daemon, urls.daemonBinary, "daemon-$channel-${composite.daemon}", composite.daemon)),
                config,
                gateway,
            )
            return
        }
        if (meta.imageVersion != composite.image) {
            runImageAuto(deviceId, composite.image, urls.imageSwu, urls.imageZck, config, gateway)
        }
    }

    private suspend fun runPollLoop(config: OtaPollConfig) {
        while (scope.isActive) {
            val gw = mutex.withLock { attachedGateway }
            if (gw != null) poll(config, gw)
            sleepUntilNextWake(config)
        }
    }

    private suspend fun sleepUntilNextWake(config: OtaPollConfig) {
        val wake = CompletableDeferred<Unit>()
        mutex.withLock { pollWake = wake }
        val now = System.currentTimeMillis()
        var deadline = now + config.intervalSeconds.coerceAtLeast(60L) * 1000L
        val soonest = mutex.withLock { autoPushNextAt.values.minOrNull() }
        if (soonest != null && soonest < deadline) {
            deadline = maxOf(soonest, now + MIN_RESUME_DELAY_MS)
        }
        val sleepMs = (deadline - now).coerceAtLeast(0L)
        withTimeoutOrNull(sleepMs) { wake.await() }
        mutex.withLock { pollWake = null }
    }

    private suspend fun wakePoll() {
        mutex.withLock { pollWake }?.complete(Unit)
    }

    private suspend fun recordMeta(deviceId: String, meta: BridgeThingMeta) {
        val isNew = deviceMetaMutex.withLock {
            val fresh = deviceMeta[deviceId] == null
            deviceMeta[deviceId] = meta
            fresh
        }
        metaChangedFlow.emit(deviceId to meta)
        if (isNew) wakePoll()
    }

    private suspend fun recordNickname(deviceId: String, nickname: String?) {
        val patched = deviceMetaMutex.withLock {
            val existing = deviceMeta[deviceId] ?: return
            existing.copy(nickname = nickname).also { deviceMeta[deviceId] = it }
        }
        metaChangedFlow.emit(deviceId to patched)
    }

    private suspend fun poll(config: OtaPollConfig, gateway: BridgethingGateway) {
        val manifest = try {
            fetchManifest("${config.rootUrl.trimEnd('/')}/manifest.json")
        } catch (e: Throwable) {
            eventsFlow.emit(OtaPollEvent.ManifestPollFailed(reason = e.message ?: e.toString()))
            return
        }
        eventsFlow.emit(OtaPollEvent.ManifestPolled(updatedAt = manifest.updatedAt))

        val channel = manifest.channels[config.channel]
        if (channel == null) {
            eventsFlow.emit(OtaPollEvent.ManifestPollFailed(reason = "configured channel '${config.channel}' not in manifest"))
            return
        }
        val composite = OtaCompositeVersion.parse(channel.latest)
        if (composite == null) {
            eventsFlow.emit(OtaPollEvent.ManifestPollFailed(reason = "channel.latest '${channel.latest}' is not a composite version"))
            return
        }
        val release = manifest.releases[channel.latest]
        if (release != null && (release.yanked != null || release.deprecated)) return

        // snapshot devices so the iteration below doesn't hold the meta lock across download work.
        val snapshot = deviceMetaMutex.withLock { deviceMeta.toMap() }
        for ((deviceId, meta) in snapshot) {
            reconcileDevice(deviceId, meta, composite, release, config, gateway)
        }
    }

    private suspend fun reconcileDevice(
        deviceId: String,
        meta: BridgeThingMeta,
        latest: OtaCompositeVersion,
        release: OtaManifestRelease?,
        config: OtaPollConfig,
        gateway: BridgethingGateway,
    ) {
        if (meta.channel != config.channel) {
            eventsFlow.emit(
                OtaPollEvent.ChannelMismatch(
                    deviceId = deviceId,
                    deviceChannel = meta.channel,
                    configuredChannel = config.channel,
                )
            )
            return
        }
        if (mutex.withLock { deviceId in inFlight }) return

        val urls = OtaArtifactUrls.build(
            rootUrl = config.rootUrl,
            channel = config.channel,
            daemonVersion = latest.daemon,
            imageVersion = latest.image,
            imageVariant = meta.imageVariant,
        )

        val batch = mutableListOf<BandaidPiece>()
        if (meta.appVersion != latest.daemon) {
            eventsFlow.emit(
                OtaPollEvent.UpdateAvailable(
                    deviceId = deviceId,
                    kind = OtaKind.Daemon,
                    fromVersion = meta.appVersion,
                    toVersion = latest.daemon,
                )
            )
            batch.add(BandaidPiece(OtaKind.Daemon, urls.daemonBinary, "daemon-${config.channel}-${latest.daemon}", latest.daemon))
        }
        for (drift in builtinWebappDrift(deviceId, release, config, gateway)) {
            eventsFlow.emit(
                OtaPollEvent.UpdateAvailable(
                    deviceId = deviceId,
                    kind = OtaKind.BuiltinWebapp,
                    fromVersion = drift.fromVersion,
                    toVersion = drift.piece.version,
                )
            )
            batch.add(drift.piece)
        }

        if (batch.isNotEmpty()) {
            if (config.autoPush && autoPushReady(deviceId)) {
                runBandaidBatchAuto(deviceId, batch, config, gateway)
            }
            return
        }

        // bandaid is current; reconcile the image (its slot carries the matching daemon + webapps).
        if (meta.imageVersion != latest.image) {
            eventsFlow.emit(
                OtaPollEvent.UpdateAvailable(
                    deviceId = deviceId,
                    kind = OtaKind.Image,
                    fromVersion = meta.imageVersion,
                    toVersion = latest.image,
                )
            )
            if (config.autoPush && autoPushReady(deviceId)) {
                runImageAuto(deviceId, latest.image, urls.imageSwu, urls.imageZck, config, gateway)
            }
        }
    }

    private data class BandaidPiece(val kind: OtaKind, val url: String, val filename: String, val version: String)

    private data class WebappDrift(val piece: BandaidPiece, val fromVersion: String)

    private suspend fun builtinWebappDrift(
        deviceId: String,
        release: OtaManifestRelease?,
        config: OtaPollConfig,
        gateway: BridgethingGateway,
    ): List<WebappDrift> {
        if (release == null || release.builtinWebapps.isEmpty()) return emptyList()
        val installed = installedWebapps(deviceId, gateway)
        val out = mutableListOf<WebappDrift>()
        for ((slug, id) in BUILTIN_WEBAPPS) {
            val available = release.builtinWebapps[slug] ?: continue
            val current = installed[id] ?: continue
            if (current == available) continue
            val url = OtaArtifactUrls.builtinWebapp(config.rootUrl, config.channel, slug, available)
            out.add(
                WebappDrift(
                    piece = BandaidPiece(OtaKind.BuiltinWebapp, url, "webapp-${config.channel}-$slug-$available", available),
                    fromVersion = current,
                )
            )
        }
        return out
    }

    private suspend fun installedWebapps(deviceId: String, gateway: BridgethingGateway): Map<UUID, String> =
        when (val r = gateway.webapp.list(deviceId)) {
            is RequestResult.Ok -> r.response.webapps.associate { it.id to it.version }
            else -> emptyMap()
        }

    private suspend fun tryBeginInFlight(deviceId: String): Boolean = mutex.withLock {
        if (deviceId in inFlight) false else { inFlight.add(deviceId); true }
    }

    private suspend fun endInFlight(deviceId: String) = mutex.withLock { inFlight.remove(deviceId) }

    private suspend fun autoPushReady(deviceId: String): Boolean = mutex.withLock {
        System.currentTimeMillis() >= (autoPushNextAt[deviceId] ?: 0L)
    }

    private suspend fun noteAutoPushResult(deviceId: String, failed: Boolean) = mutex.withLock {
        if (failed) {
            val n = (autoPushFailures[deviceId] ?: 0) + 1
            autoPushFailures[deviceId] = n
            val delay = (AUTO_PUSH_BACKOFF_BASE_MS shl (n - 1).coerceAtMost(5)).coerceAtMost(AUTO_PUSH_BACKOFF_MAX_MS)
            autoPushNextAt[deviceId] = System.currentTimeMillis() + delay
        } else {
            autoPushFailures.remove(deviceId)
            autoPushNextAt.remove(deviceId)
        }
    }

    private suspend fun runBandaidBatchAuto(
        deviceId: String,
        pieces: List<BandaidPiece>,
        config: OtaPollConfig,
        gateway: BridgethingGateway,
    ) {
        if (pieces.isEmpty()) return
        if (!tryBeginInFlight(deviceId)) return
        try {
            val cacheDir = effectiveCacheDir(config)
            val artifacts = mutableListOf<Pair<OtaKind, File>>()
            for (piece in pieces) {
                val cached = try {
                    downloadIfNeeded(piece.url, cacheDir, piece.filename)
                } catch (e: Throwable) {
                    eventsFlow.emit(
                        OtaPollEvent.Failed(
                            deviceId = deviceId,
                            kind = piece.kind,
                            reason = "bandaid download failed: ${e.message ?: e.toString()}",
                        )
                    )
                    noteAutoPushResult(deviceId, failed = true)
                    return
                }
                artifacts.add(piece.kind to cached)
            }
            val labelKind = if (pieces.any { it.kind == OtaKind.Daemon }) OtaKind.Daemon else OtaKind.BuiltinWebapp
            var last: OtaPhaseSnapshot = OtaPhaseSnapshot.Idle
            val terminal = applyBandaidBatch(
                gateway = gateway,
                deviceId = deviceId,
                artifacts = artifacts,
                emit = { snapshot ->
                    last = snapshot
                    eventsFlow.tryEmit(
                        OtaPollEvent.Progress(deviceId = deviceId, kind = labelKind, snapshot = snapshot)
                    )
                },
            )
            val finalSnap = terminal.takeUnless { it is OtaPhaseSnapshot.Idle } ?: last
            if (finalSnap is OtaPhaseSnapshot.Failed) {
                eventsFlow.emit(OtaPollEvent.Failed(deviceId = deviceId, kind = labelKind, reason = finalSnap.reason))
                noteAutoPushResult(deviceId, failed = true)
            } else {
                for (piece in pieces) {
                    eventsFlow.emit(OtaPollEvent.Updated(deviceId = deviceId, kind = piece.kind, version = piece.version))
                }
                noteAutoPushResult(deviceId, failed = false)
            }
        } finally {
            endInFlight(deviceId)
        }
    }

    private suspend fun runImageAuto(
        deviceId: String,
        targetVersion: String,
        swuUrl: String,
        zckUrl: String,
        config: OtaPollConfig,
        gateway: BridgethingGateway,
    ) {
        if (!tryBeginInFlight(deviceId)) return
        try {
            val cacheDir = effectiveCacheDir(config)
            val swuLocal: File
            val zckLocal: File
            try {
                swuLocal = downloadIfNeeded(swuUrl, cacheDir, "image-${config.channel}-$targetVersion.swu")
                zckLocal = downloadIfNeeded(zckUrl, cacheDir, "image-${config.channel}-$targetVersion.zck")
            } catch (e: Throwable) {
                eventsFlow.emit(
                    OtaPollEvent.Failed(
                        deviceId = deviceId,
                        kind = OtaKind.Image,
                        reason = "image download failed: ${e.message ?: e.toString()}",
                    )
                )
                return
            }
            setLocalZck(zckLocal)
            var last: OtaPhaseSnapshot = OtaPhaseSnapshot.Idle
            val (terminal, _) = driveOta(
                gateway = gateway,
                deviceId = deviceId,
                kind = OtaKind.Image,
                artifactPath = swuLocal,
                updateUrlBase = config.rootUrl,
                mode = DriveMode.Full,
                emit = { snapshot ->
                    last = snapshot
                    eventsFlow.tryEmit(
                        OtaPollEvent.Progress(deviceId = deviceId, kind = OtaKind.Image, snapshot = snapshot)
                    )
                },
            )
            val finalSnap = terminal.takeUnless { it is OtaPhaseSnapshot.Idle } ?: last
            emitTerminal(deviceId, OtaKind.Image, targetVersion, finalSnap)
            noteAutoPushResult(deviceId, failed = finalSnap is OtaPhaseSnapshot.Failed)
        } finally {
            endInFlight(deviceId)
        }
    }

    private suspend fun emitTerminal(
        deviceId: String,
        kind: OtaKind,
        version: String,
        terminal: OtaPhaseSnapshot,
    ) {
        when (terminal) {
            is OtaPhaseSnapshot.Failed -> eventsFlow.emit(
                OtaPollEvent.Failed(deviceId = deviceId, kind = kind, reason = terminal.reason)
            )
            else -> eventsFlow.emit(OtaPollEvent.Updated(deviceId = deviceId, kind = kind, version = version))
        }
    }

    private fun effectiveCacheDir(config: OtaPollConfig): File {
        val base = config.cacheDirectory ?: File(System.getProperty("java.io.tmpdir") ?: "/tmp")
        val dir = File(base, "bridgething-ota")
        if (!dir.exists()) dir.mkdirs()
        return dir
    }

    private suspend fun fetchManifest(url: String): OtaDiscoverManifest = withContext(Dispatchers.IO) {
        val req = Request.Builder().url(url).build()
        httpClient.newCall(req).execute().use { resp ->
            if (!resp.isSuccessful) throw IOException("manifest fetch returned HTTP ${resp.code}")
            val body = resp.body?.string() ?: throw IOException("manifest fetch returned empty body")
            json.decodeFromString(OtaDiscoverManifest.serializer(), body)
        }
    }

    private suspend fun downloadIfNeeded(url: String, dir: File, filename: String): File = withContext(Dispatchers.IO) {
        if (!dir.exists()) dir.mkdirs()
        val target = File(dir, filename)
        if (target.exists() && target.length() > 0L) return@withContext target
        val req = Request.Builder().url(url).build()
        httpClient.newCall(req).execute().use { resp ->
            if (!resp.isSuccessful) throw IOException("artifact fetch returned HTTP ${resp.code}")
            val body = resp.body ?: throw IOException("artifact fetch returned empty body")
            target.outputStream().use { out ->
                body.byteStream().use { input -> input.copyTo(out) }
            }
        }
        target
    }

    private suspend fun handleRangeRequest(
        gateway: BridgethingGateway,
        handle: OtaAssetRangeHandle,
        req: OtaAssetRange,
    ) {
        val zck = localZck
        if (zck == null) {
            runCatching { handle.respondErr(OtaAssetRangeRejected(reason = "companion has no .zck cached")) }
            return
        }
        val length = try { zck.length() } catch (e: Throwable) {
            runCatching { handle.respondErr(OtaAssetRangeRejected(reason = "stat zck failed: ${e.message ?: e.toString()}")) }
            return
        }
        if (length <= 0L || length > UInt.MAX_VALUE.toLong()) {
            runCatching { handle.respondErr(OtaAssetRangeRejected(reason = "zck size unavailable or > 4 GiB")) }
            return
        }
        val totalSize = length.toUInt()
        for (r in req.ranges) {
            val end = r.start.toLong() + r.length.toLong()
            if (end > totalSize.toLong()) {
                runCatching {
                    handle.respondErr(OtaAssetRangeRejected(reason = "range ${r.start}+${r.length} exceeds zck size $totalSize"))
                }
                return
            }
        }
        val parts = req.ranges.map { RangePart(start = it.start, length = it.length) }
        val streamLen = parts.fold(0u) { acc, p -> acc + p.length }

        val raf = try { RandomAccessFile(zck, "r") } catch (_: Throwable) {
            runCatching { handle.respondErr(OtaAssetRangeRejected(reason = "open zck failed")) }
            return
        }
        try {
            if (streamLen <= INLINE_RANGE_MAX_BYTES) {
                val body = ByteArray(streamLen.toInt())
                var at = 0
                for (part in parts) {
                    try {
                        raf.seek(part.start.toLong())
                        raf.readFully(body, at, part.length.toInt())
                    } catch (_: Throwable) {
                        runCatching { handle.respondErr(OtaAssetRangeRejected(reason = "read zck failed")) }
                        return
                    }
                    at += part.length.toInt()
                }
                runCatching {
                    handle.respond(OtaAssetRangeReply(totalSize = totalSize, parts = parts, body = TransferBody.Inline(body)))
                }
                return
            }

            try {
                handle.respond(
                    OtaAssetRangeReply(
                        totalSize = totalSize,
                        parts = parts,
                        body = TransferBody.Stream(
                            TransferRef(id = handle.requestId, totalSize = streamLen, sha256 = null),
                        ),
                    ),
                )
            } catch (_: Throwable) {
                return
            }

            val chunkBytes = 64 * 1024
            var streamOffset: UInt = 0u
            for (part in parts) {
                try { raf.seek(part.start.toLong()) } catch (_: Throwable) { return }
                var produced: UInt = 0u
                while (produced < part.length) {
                    val want = minOf(chunkBytes.toLong(), (part.length - produced).toLong()).toInt()
                    val buf = ByteArray(want)
                    val read = try { raf.read(buf) } catch (_: Throwable) { return }
                    if (read <= 0) return
                    val data = if (read == buf.size) buf else buf.copyOf(read)
                    produced = (produced.toLong() + read.toLong())
                        .coerceAtMost(UInt.MAX_VALUE.toLong()).toUInt()
                    try {
                        gateway.device(handle.deviceId).transfer.fragment(
                            TransferFragment(transferId = handle.requestId, offset = streamOffset, bytes = data),
                            priority = Priority.Background,
                        )
                    } catch (_: Throwable) {
                        return
                    }
                    streamOffset += read.toUInt()
                }
            }
        } finally {
            runCatching { raf.close() }
        }
    }

    private enum class DriveMode {
        /** Image: stream, then await `Reboot` (the device power-cycles). */
        Full,

        /**
         * Bandaid (daemon / builtin-webapp): stream, then await `Writing`/100,
         * which means the piece is staged but not yet live. The batch goes
         * live on a later `OtaActivate`.
         */
        Stage,
    }

    /**
     * Stream one artifact and await its terminal. Returns the terminal
     * snapshot plus the artifact's sha256 (the `update_id`), which the caller
     * passes to `OtaActivate.expected` for a bandaid batch.
     */
    private suspend fun driveOta(
        gateway: BridgethingGateway,
        deviceId: String,
        kind: OtaKind,
        artifactPath: File,
        updateUrlBase: String?,
        mode: DriveMode,
        emit: suspend (OtaPhaseSnapshot) -> Unit,
    ): Pair<OtaPhaseSnapshot, String> {
        val totalSize = try { artifactPath.length() } catch (e: Throwable) {
            return OtaPhaseSnapshot.Failed(reason = "stat artifact failed: ${e.message ?: e.toString()}") to ""
        }
        if (totalSize <= 0L) {
            return OtaPhaseSnapshot.Failed(reason = "could not stat artifact") to ""
        }
        if (totalSize > UInt.MAX_VALUE.toLong()) {
            return OtaPhaseSnapshot.Failed(reason = "artifact larger than 4 GiB") to ""
        }

        val sha = try { hashFile(artifactPath) } catch (e: Throwable) {
            return OtaPhaseSnapshot.Failed(reason = "sha256 failed: ${e.message ?: e.toString()}") to ""
        }

        val transferId = UUID.randomUUID()
        val begin = OtaBegin(
            kind = kind,
            updateId = sha,
            updateUrlBase = updateUrlBase,
            transfer = TransferRef(id = transferId, totalSize = totalSize.toUInt(), sha256 = sha),
        )
        val resumeFrom: UInt = try {
            when (val res = gateway.device(deviceId).system.otaBegin(begin)) {
                is RequestResult.Ok -> res.response.resumeFromOffset
                is RequestResult.DomainErr -> {
                    return OtaPhaseSnapshot.Failed(reason = "daemon rejected OtaBegin: ${res.error.reason}") to sha
                }
                is RequestResult.ProtocolErr -> {
                    return OtaPhaseSnapshot.Failed(reason = "OtaBegin protocol error: ${res.error}") to sha
                }
            }
        } catch (e: Throwable) {
            return OtaPhaseSnapshot.Failed(reason = "OtaBegin send failed: ${e.message ?: e.toString()}") to sha
        }

        emit(OtaPhaseSnapshot.Streaming(percent = percent(resumeFrom.toLong(), totalSize)))

        val success: OtaPhaseSnapshot = if (mode == DriveMode.Full) OtaPhaseSnapshot.Completed else OtaPhaseSnapshot.Staged
        val terminal = CompletableDeferred<OtaPhaseSnapshot>()
        val progressJob = scope.launch {
            gateway.device(deviceId).system.otaProgress.collect { p ->
                emit(OtaPhaseSnapshot.Applying(phase = p.phase, percent = p.percent.toInt()))
                val done = when (mode) {
                    DriveMode.Full -> p.phase == OtaPhase.Reboot
                    DriveMode.Stage -> p.phase == OtaPhase.Writing && p.percent.toInt() >= 100
                }
                if (done && terminal.isActive) terminal.complete(success)
            }
        }
        val errorJob = scope.launch {
            gateway.device(deviceId).system.otaError.collect { e ->
                if (terminal.isActive) {
                    terminal.complete(OtaPhaseSnapshot.Failed(reason = "[${e.code}] ${e.msg}"))
                }
            }
        }

        val streamJob = scope.launch {
            try {
                streamArtifact(
                    gateway = gateway,
                    deviceId = deviceId,
                    transferId = transferId,
                    artifactPath = artifactPath,
                    startOffset = resumeFrom.toLong(),
                    totalSize = totalSize,
                )
            } catch (e: CancellationException) {
                throw e
            } catch (e: Throwable) {
                if (terminal.isActive) {
                    terminal.complete(OtaPhaseSnapshot.Failed(reason = "chunk stream failed: ${e.message ?: e.toString()}"))
                }
            }
        }

        val result = try { terminal.await() } finally {
            streamJob.cancelAndJoin()
            progressJob.cancelAndJoin()
            errorJob.cancelAndJoin()
        }
        if (result is OtaPhaseSnapshot.Failed) {
            runCatching {
                gateway.device(deviceId).transfer.abandon(TransferAbandon(transferId = transferId, reason = "attempt ended"))
            }
        }
        return result to sha
    }

    private suspend fun applyBandaidBatch(
        gateway: BridgethingGateway,
        deviceId: String,
        artifacts: List<Pair<OtaKind, File>>,
        emit: suspend (OtaPhaseSnapshot) -> Unit,
    ): OtaPhaseSnapshot {
        val stagedIds = mutableListOf<String>()
        for ((kind, path) in artifacts) {
            val (snapshot, updateId) = driveOta(
                gateway = gateway,
                deviceId = deviceId,
                kind = kind,
                artifactPath = path,
                updateUrlBase = null,
                mode = DriveMode.Stage,
                emit = emit,
            )
            if (snapshot !is OtaPhaseSnapshot.Staged) return snapshot
            stagedIds.add(updateId)
        }
        return commitBandaid(gateway, deviceId, stagedIds, emit)
    }

    private suspend fun commitBandaid(
        gateway: BridgethingGateway,
        deviceId: String,
        expected: List<String>,
        emit: suspend (OtaPhaseSnapshot) -> Unit,
    ): OtaPhaseSnapshot {
        val terminal = CompletableDeferred<OtaPhaseSnapshot>()
        val progressJob = scope.launch {
            gateway.device(deviceId).system.otaProgress.collect { p ->
                emit(OtaPhaseSnapshot.Applying(phase = p.phase, percent = p.percent.toInt()))
                if (p.phase == OtaPhase.Reboot && terminal.isActive) terminal.complete(OtaPhaseSnapshot.Completed)
            }
        }
        val errorJob = scope.launch {
            gateway.device(deviceId).system.otaError.collect { e ->
                if (terminal.isActive) terminal.complete(OtaPhaseSnapshot.Failed(reason = "[${e.code}] ${e.msg}"))
            }
        }
        try {
            gateway.device(deviceId).system.otaActivate(OtaActivate(expected = expected))
        } catch (e: Throwable) {
            progressJob.cancel(); errorJob.cancel()
            return OtaPhaseSnapshot.Failed(reason = "OtaActivate send failed: ${e.message ?: e.toString()}")
        }
        return try { terminal.await() } finally {
            progressJob.cancelAndJoin()
            errorJob.cancelAndJoin()
        }
    }

    private suspend fun streamArtifact(
        gateway: BridgethingGateway,
        deviceId: String,
        transferId: UUID,
        artifactPath: File,
        startOffset: Long,
        totalSize: Long,
    ) = withContext(Dispatchers.IO) {
        try {
            RandomAccessFile(artifactPath, "r").use { raf ->
                if (startOffset > 0L) raf.seek(startOffset)
                var offset = startOffset
                while (offset < totalSize) {
                    // hold no more than OTA_WINDOW_BYTES unacked so a cancelled attempt leaves nothing in flight.
                    while (true) {
                        val acked = transferAcks.receivedBytes(transferId).toLong()
                        if (offset < acked + OTA_WINDOW_BYTES) break
                        if (!transferAcks.waitForProgress(transferId, acked.toUInt(), OTA_ACK_TIMEOUT_MS)) {
                            throw IOException("transfer stalled: fragment acks stopped at $offset/$totalSize")
                        }
                    }
                    val want = minOf(OTA_FRAGMENT_BYTES.toLong(), totalSize - offset).toInt()
                    val buf = ByteArray(want)
                    val read = raf.read(buf)
                    if (read <= 0) throw IOException("EOF at $offset/$totalSize before last fragment")
                    val data = if (read == buf.size) buf else buf.copyOf(read)
                    gateway.device(deviceId).transfer.fragment(
                        TransferFragment(transferId = transferId, offset = offset.toUInt(), bytes = data),
                        priority = Priority.Background,
                    )
                    offset += read
                }
            }
        } finally {
            transferAcks.finish(transferId)
        }
    }

    private suspend fun hashFile(file: File): String = withContext(Dispatchers.IO) {
        val md = MessageDigest.getInstance("SHA-256")
        file.inputStream().use { input ->
            val buf = ByteArray(64 * 1024)
            while (true) {
                val n = input.read(buf)
                if (n <= 0) break
                md.update(buf, 0, n)
            }
        }
        md.digest().joinToString("") { String.format("%02x", it) }
    }

    private fun percent(n: Long, d: Long): Int {
        if (d <= 0L) return 100
        val p = (n * 100L) / d
        return p.coerceIn(0L, 100L).toInt()
    }

    private inline fun runOtaFlow(
        crossinline block: suspend (suspend (OtaPhaseSnapshot) -> Unit) -> Unit,
    ): Flow<OtaPhaseSnapshot> = kotlinx.coroutines.flow.flow {
        block { snapshot -> emit(snapshot) }
    }

    public fun close() {
        scope.cancel()
    }

    private companion object {
        val INLINE_RANGE_MAX_BYTES: UInt = 16u * 1024u

        const val OTA_FRAGMENT_BYTES = 4 * 1024
        const val OTA_WINDOW_BYTES = 32 * 1024L
        const val OTA_ACK_TIMEOUT_MS = 15_000L

        const val AUTO_PUSH_BACKOFF_BASE_MS = 30_000L
        const val AUTO_PUSH_BACKOFF_MAX_MS = 15L * 60L * 1000L
        const val MIN_RESUME_DELAY_MS = 5_000L

        val BUILTIN_WEBAPPS: List<Pair<String, UUID>> = listOf(
            "hub" to UUID.fromString("019693c0-5c6a-71f0-a89d-7e2a4d9c0a01"),
            "stock" to UUID.fromString("b12be731-416c-4cf7-8a91-3d2f19a45e21"),
        )

        val defaultJson: Json = Json {
            ignoreUnknownKeys = true
            isLenient = true
        }
    }
}
