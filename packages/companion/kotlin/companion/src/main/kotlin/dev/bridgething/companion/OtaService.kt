package dev.bridgething.companion

import dev.bridgething.gateway.BridgethingGateway
import dev.bridgething.gateway.GatewayEvent
import dev.bridgething.gateway.OtaAssetRangeHandle
import dev.bridgething.gateway.RequestResult
import dev.bridgething.gateway.device
import dev.bridgething.gateway.system
import dev.bridgething.schema.BridgeThingMeta
import dev.bridgething.schema.BridgeToGatewayMsgData
import dev.bridgething.schema.OtaAssetRange
import dev.bridgething.schema.OtaAssetRangeChunk
import dev.bridgething.schema.OtaAssetRangeRejected
import dev.bridgething.schema.OtaAssetRangeReply
import dev.bridgething.schema.OtaBegin
import dev.bridgething.schema.OtaChunk
import dev.bridgething.schema.OtaKind
import dev.bridgething.schema.OtaPhase
import dev.bridgething.schema.Priority
import dev.bridgething.schema.RangePart
import java.io.File
import java.io.IOException
import java.io.RandomAccessFile
import java.security.MessageDigest
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
import kotlinx.serialization.json.Json
import okhttp3.OkHttpClient
import okhttp3.Request

/**
 * Snapshot of the current OTA flow visible to the host app's UI.
 * Mirror of Swift `OtaPhaseSnapshot`.
 */
public sealed class OtaPhaseSnapshot {
    public object Idle : OtaPhaseSnapshot()
    public data class Streaming(val percent: Int) : OtaPhaseSnapshot()
    public data class Applying(val phase: OtaPhase, val percent: Int) : OtaPhaseSnapshot()
    public object Completed : OtaPhaseSnapshot()
    public data class Failed(val reason: String) : OtaPhaseSnapshot()
}

/**
 * Configuration for the manifest poll loop. Mirror of Swift `OtaPollConfig`.
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
 * [Progress] carrying an [OtaPhaseSnapshot]. Mirror of Swift
 * `OtaPollEvent`.
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
 * OTA service for the bridgething companion. Mirror of Swift `OtaService`.
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
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val mutex = Mutex()
    private val deviceMetaMutex = Mutex()

    private var localZck: File? = null
    private var rangeServerJob: Job? = null
    private var metaJob: Job? = null
    private var pollJob: Job? = null

    private var attachedGateway: BridgethingGateway? = null
    private var pollConfig: OtaPollConfig? = null
    private val deviceMeta = mutableMapOf<String, BridgeThingMeta>()
    private val inFlight = mutableSetOf<String>()

    private val eventsFlow = MutableSharedFlow<OtaPollEvent>(
        extraBufferCapacity = 64,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )

    /** High-level poll-loop events. The host app drives UI off this stream. */
    public val events: Flow<OtaPollEvent> = eventsFlow.asSharedFlow()

    /**
     * Start serving inbound `OtaAssetRange` requests and tracking
     * per-device [BridgeThingMeta]. Safe to call again after [stop].
     */
    public suspend fun start(gateway: BridgethingGateway) {
        mutex.withLock {
            attachedGateway = gateway
            rangeServerJob?.cancel()
            metaJob?.cancel()
            rangeServerJob = scope.launch {
                gateway.system.otaAssetRangeRequests.collect { (handle, req) ->
                    launch { handleRangeRequest(gateway, handle, req) }
                }
            }
            metaJob = scope.launch {
                gateway.events.collect { event ->
                    if (event !is GatewayEvent.Message) return@collect
                    val data = event.message.data
                    if (data is BridgeToGatewayMsgData.Version) {
                        recordMeta(event.deviceId, data.data)
                    }
                }
            }
        }
    }

    public suspend fun stop() {
        mutex.withLock {
            rangeServerJob?.cancel(); rangeServerJob = null
            metaJob?.cancel(); metaJob = null
            pollJob?.cancel(); pollJob = null
            attachedGateway = null
        }
        deviceMetaMutex.withLock { deviceMeta.clear() }
    }

    public fun setLocalZck(file: File?) {
        localZck = file
    }

    public fun currentLocalZck(): File? = localZck

    /**
     * Drive an image-kind OTA from a local `.swu` (and matching `.zck`
     * for delta fetch). Returns a flow of [OtaPhaseSnapshot]; finishes
     * on terminal state. `updateUrlBase` is recorded on `OtaBegin.update_url_base`
     * for future cache-miss recovery flows.
     */
    public suspend fun pushUpdate(
        gateway: BridgethingGateway,
        deviceId: String,
        swuPath: File,
        zckPath: File,
        updateUrlBase: String? = null,
    ): Flow<OtaPhaseSnapshot> {
        setLocalZck(zckPath)
        return runOtaFlow { collector ->
            val terminal = driveOta(
                gateway = gateway,
                deviceId = deviceId,
                kind = OtaKind.Image,
                artifactPath = swuPath,
                updateUrlBase = updateUrlBase,
                emit = collector,
            )
            collector(terminal)
        }
    }

    /**
     * Drive a daemon-kind OTA from a local aarch64 daemon binary. The
     * daemon validates and atomically swaps `.current`, then a
     * `systemctl restart bridgething.service` triggers the new binary
     * to take over. No range proxy traffic for this kind.
     */
    public suspend fun pushDaemon(
        gateway: BridgethingGateway,
        deviceId: String,
        binaryPath: File,
    ): Flow<OtaPhaseSnapshot> {
        return runOtaFlow { collector ->
            val terminal = driveOta(
                gateway = gateway,
                deviceId = deviceId,
                kind = OtaKind.Daemon,
                artifactPath = binaryPath,
                updateUrlBase = null,
                emit = collector,
            )
            collector(terminal)
        }
    }

    // MARK: - manifest poll loop

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

    private suspend fun runPollLoop(config: OtaPollConfig) {
        while (scope.isActive) {
            val gw = mutex.withLock { attachedGateway }
            if (gw != null) poll(config, gw)
            delay(config.intervalSeconds.coerceAtLeast(60L) * 1000L)
        }
    }

    private suspend fun recordMeta(deviceId: String, meta: BridgeThingMeta) {
        deviceMetaMutex.withLock { deviceMeta[deviceId] = meta }
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
        manifest.releases[channel.latest]?.let { release ->
            if (release.yanked != null || release.deprecated) return
        }

        // Snapshot devices so the iteration below doesn't hold the
        // meta lock across per-device download work.
        val snapshot = deviceMetaMutex.withLock { deviceMeta.toMap() }
        for ((deviceId, meta) in snapshot) {
            reconcileDevice(deviceId, meta, composite, config, gateway)
        }
    }

    private suspend fun reconcileDevice(
        deviceId: String,
        meta: BridgeThingMeta,
        latest: OtaCompositeVersion,
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

        if (meta.appVersion != latest.daemon) {
            eventsFlow.emit(
                OtaPollEvent.UpdateAvailable(
                    deviceId = deviceId,
                    kind = OtaKind.Daemon,
                    fromVersion = meta.appVersion,
                    toVersion = latest.daemon,
                )
            )
            if (config.autoPush) {
                runDaemonAuto(deviceId, latest.daemon, urls.daemonBinary, config, gateway)
            }
            // Daemon push restarts the gateway link; let the next poll
            // cycle handle the image check when the link is back.
            return
        }

        if (meta.imageVersion != latest.image) {
            eventsFlow.emit(
                OtaPollEvent.UpdateAvailable(
                    deviceId = deviceId,
                    kind = OtaKind.Image,
                    fromVersion = meta.imageVersion,
                    toVersion = latest.image,
                )
            )
            if (config.autoPush) {
                runImageAuto(deviceId, latest.image, urls.imageSwu, urls.imageZck, config, gateway)
            }
        }
    }

    private suspend fun runDaemonAuto(
        deviceId: String,
        targetVersion: String,
        binaryUrl: String,
        config: OtaPollConfig,
        gateway: BridgethingGateway,
    ) {
        mutex.withLock { inFlight.add(deviceId) }
        try {
            val cacheDir = effectiveCacheDir(config)
            val cached = try {
                downloadIfNeeded(binaryUrl, cacheDir, "daemon-${config.channel}-$targetVersion")
            } catch (e: Throwable) {
                eventsFlow.emit(
                    OtaPollEvent.Failed(
                        deviceId = deviceId,
                        kind = OtaKind.Daemon,
                        reason = "daemon download failed: ${e.message ?: e.toString()}",
                    )
                )
                return
            }
            var last: OtaPhaseSnapshot = OtaPhaseSnapshot.Idle
            val terminal = driveOta(
                gateway = gateway,
                deviceId = deviceId,
                kind = OtaKind.Daemon,
                artifactPath = cached,
                updateUrlBase = null,
                emit = { snapshot ->
                    last = snapshot
                    eventsFlow.tryEmit(
                        OtaPollEvent.Progress(deviceId = deviceId, kind = OtaKind.Daemon, snapshot = snapshot)
                    )
                },
            )
            emitTerminal(deviceId, OtaKind.Daemon, targetVersion, terminal.takeUnless { it is OtaPhaseSnapshot.Idle } ?: last)
        } finally {
            mutex.withLock { inFlight.remove(deviceId) }
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
        mutex.withLock { inFlight.add(deviceId) }
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
            val terminal = driveOta(
                gateway = gateway,
                deviceId = deviceId,
                kind = OtaKind.Image,
                artifactPath = swuLocal,
                updateUrlBase = config.rootUrl,
                emit = { snapshot ->
                    last = snapshot
                    eventsFlow.tryEmit(
                        OtaPollEvent.Progress(deviceId = deviceId, kind = OtaKind.Image, snapshot = snapshot)
                    )
                },
            )
            emitTerminal(deviceId, OtaKind.Image, targetVersion, terminal.takeUnless { it is OtaPhaseSnapshot.Idle } ?: last)
        } finally {
            mutex.withLock { inFlight.remove(deviceId) }
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

    // MARK: - inbound range serving

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
        try {
            handle.respond(OtaAssetRangeReply(totalSize = totalSize, parts = parts))
        } catch (_: Throwable) {
            return
        }

        val raf = try { RandomAccessFile(zck, "r") } catch (_: Throwable) { return }
        try {
            val chunkBytes = 64 * 1024
            for ((idx, part) in parts.withIndex()) {
                try { raf.seek(part.start.toLong()) } catch (_: Throwable) { return }
                var produced: UInt = 0u
                while (produced < part.length) {
                    val want = minOf(chunkBytes.toLong(), (part.length - produced).toLong()).toInt()
                    val buf = ByteArray(want)
                    val read = try { raf.read(buf) } catch (_: Throwable) { return }
                    if (read <= 0) return
                    val data = if (read == buf.size) buf else buf.copyOf(read)
                    val absOffset = part.start + produced
                    produced = (produced.toLong() + read.toLong())
                        .coerceAtMost(UInt.MAX_VALUE.toLong()).toUInt()
                    val last = (idx + 1 == parts.size) && produced == part.length
                    val chunk = OtaAssetRangeChunk(
                        requestId = handle.requestId,
                        partIndex = idx.toUInt(),
                        offset = absOffset,
                        bytes = data,
                        last = last,
                    )
                    try {
                        gateway.device(handle.deviceId).system.otaAssetRangeChunk(chunk, priority = Priority.Bulk)
                    } catch (_: Throwable) {
                        return
                    }
                }
            }
        } finally {
            runCatching { raf.close() }
        }
    }

    // MARK: - push-side driver

    private suspend fun driveOta(
        gateway: BridgethingGateway,
        deviceId: String,
        kind: OtaKind,
        artifactPath: File,
        updateUrlBase: String?,
        emit: suspend (OtaPhaseSnapshot) -> Unit,
    ): OtaPhaseSnapshot {
        val totalSize = try { artifactPath.length() } catch (e: Throwable) {
            return OtaPhaseSnapshot.Failed(reason = "stat artifact failed: ${e.message ?: e.toString()}")
        }
        if (totalSize <= 0L) {
            return OtaPhaseSnapshot.Failed(reason = "could not stat artifact")
        }
        if (totalSize > UInt.MAX_VALUE.toLong()) {
            return OtaPhaseSnapshot.Failed(reason = "artifact larger than 4 GiB")
        }

        val sha = try { hashFile(artifactPath) } catch (e: Throwable) {
            return OtaPhaseSnapshot.Failed(reason = "sha256 failed: ${e.message ?: e.toString()}")
        }

        val begin = OtaBegin(
            kind = kind,
            updateId = sha,
            updateUrlBase = updateUrlBase,
            expectedSha256 = sha,
            expectedSize = totalSize.toUInt(),
        )
        val resumeFrom: UInt = try {
            when (val res = gateway.device(deviceId).system.otaBegin(begin)) {
                is RequestResult.Ok -> res.response.resumeFromOffset
                is RequestResult.DomainErr -> {
                    return OtaPhaseSnapshot.Failed(reason = "daemon rejected OtaBegin: ${res.error.reason}")
                }
                is RequestResult.ProtocolErr -> {
                    return OtaPhaseSnapshot.Failed(reason = "OtaBegin protocol error: ${res.error}")
                }
            }
        } catch (e: Throwable) {
            return OtaPhaseSnapshot.Failed(reason = "OtaBegin send failed: ${e.message ?: e.toString()}")
        }

        emit(OtaPhaseSnapshot.Streaming(percent = percent(resumeFrom.toLong(), totalSize)))

        val terminal = CompletableDeferred<OtaPhaseSnapshot>()
        val progressJob = scope.launch {
            gateway.device(deviceId).system.otaProgress.collect { p ->
                emit(OtaPhaseSnapshot.Applying(phase = p.phase, percent = p.percent.toInt()))
                if (p.phase == OtaPhase.Reboot) {
                    if (terminal.isActive) terminal.complete(OtaPhaseSnapshot.Completed)
                }
            }
        }
        val errorJob = scope.launch {
            gateway.device(deviceId).system.otaError.collect { e ->
                if (terminal.isActive) {
                    terminal.complete(OtaPhaseSnapshot.Failed(reason = "[${e.code}] ${e.msg}"))
                }
            }
        }

        try {
            streamArtifact(
                gateway = gateway,
                deviceId = deviceId,
                updateId = sha,
                artifactPath = artifactPath,
                startOffset = resumeFrom.toLong(),
                totalSize = totalSize,
            )
        } catch (e: Throwable) {
            progressJob.cancel(); errorJob.cancel()
            return OtaPhaseSnapshot.Failed(reason = "chunk stream failed: ${e.message ?: e.toString()}")
        }

        val result = try { terminal.await() } finally {
            progressJob.cancelAndJoin()
            errorJob.cancelAndJoin()
        }
        return result
    }

    private suspend fun streamArtifact(
        gateway: BridgethingGateway,
        deviceId: String,
        updateId: String,
        artifactPath: File,
        startOffset: Long,
        totalSize: Long,
    ) = withContext(Dispatchers.IO) {
        val chunkSize = 64 * 1024
        RandomAccessFile(artifactPath, "r").use { raf ->
            if (startOffset > 0L) raf.seek(startOffset)
            var offset = startOffset
            while (offset < totalSize) {
                val want = minOf(chunkSize.toLong(), totalSize - offset).toInt()
                val buf = ByteArray(want)
                val read = raf.read(buf)
                if (read <= 0) throw IOException("EOF at $offset/$totalSize before last chunk")
                val data = if (read == buf.size) buf else buf.copyOf(read)
                val last = offset + read == totalSize
                val chunk = OtaChunk(
                    updateId = updateId,
                    offset = offset.toUInt(),
                    bytes = data,
                    last = last,
                )
                gateway.device(deviceId).system.otaChunk(chunk, priority = Priority.Bulk)
                offset += read
            }
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
        val defaultJson: Json = Json {
            ignoreUnknownKeys = true
            isLenient = true
        }
    }
}
