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
import com.bridgething.schema.OtaPatch
import com.bridgething.schema.OtaPatchAlgorithm
import com.bridgething.schema.OtaPhase
import com.bridgething.schema.OtaProgress
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
import kotlinx.coroutines.NonCancellable
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

public sealed class OtaPhaseSnapshot {
    public object Idle : OtaPhaseSnapshot()

    public data class Downloading(
        val asset: String,
        val received: Long,
        val total: Long,
        val ratePerSec: Double?,
    ) : OtaPhaseSnapshot()

    public data class Streaming(
        val asset: String,
        val sent: Long,
        val total: Long,
        val ratePerSec: Double?,
        val etaSeconds: Double?,
    ) : OtaPhaseSnapshot()

    public data class Applying(
        val phase: OtaPhase,
        val writePercent: Int,
        val dwlPercent: Int,
        val dwlBytes: Long,
    ) : OtaPhaseSnapshot()

    public object Staged : OtaPhaseSnapshot()
    public object Completed : OtaPhaseSnapshot()
    public data class Failed(val reason: String) : OtaPhaseSnapshot()
}

public enum class OtaStepKind { DOWNLOAD, STREAM, APPLY, REBOOT }

public data class OtaPlanStep(
    val id: Int,
    val kind: OtaStepKind,
    val label: String,
    val bytes: Long,
)

internal class RateTracker(private val windowMs: Long = 4_000L) {
    private data class Sample(val bytes: Long, val atMs: Long)

    private val samples = mutableListOf<Sample>()
    private val lock = Any()

    fun record(bytes: Long) {
        synchronized(lock) {
            val now = System.currentTimeMillis()
            samples.add(Sample(bytes, now))
            val cutoff = now - windowMs
            while (samples.size > 2 && samples.first().atMs < cutoff) {
                samples.removeAt(0)
            }
        }
    }

    fun ratePerSec(): Double? = synchronized(lock) {
        val first = samples.firstOrNull() ?: return@synchronized null
        val last = samples.lastOrNull() ?: return@synchronized null
        val dtMs = last.atMs - first.atMs
        if (dtMs <= 50L || last.bytes < first.bytes) return@synchronized null
        (last.bytes - first.bytes).toDouble() / (dtMs / 1000.0)
    }

    fun etaSeconds(remaining: Long): Double? {
        val rate = ratePerSec() ?: return null
        if (rate <= 0.0) return null
        return remaining / rate
    }
}

private class DigestMismatchException(asset: String, field: String) :
    IOException("$asset $field does not match the manifest; refusing to install")

public sealed class WebappInstallResult {
    public data class Installed(val info: WebappInfo) : WebappInstallResult()
    public data class Failed(val reason: String) : WebappInstallResult()
}

public data class OtaPollConfig(
    val rootUrl: String = "https://ota.bridgething.com",
    val intervalSeconds: Long = 60 * 60L,
    val cacheDirectory: File? = null,
    val autoPush: Boolean = true,
)

public sealed class OtaPollEvent {
    public data class ManifestPolled(val updatedAt: String) : OtaPollEvent()
    public data class ManifestPollFailed(val reason: String) : OtaPollEvent()
    public data class UpdateAvailable(
        val deviceId: String,
        val release: String,
        val daemonVersion: String,
        val imageVersion: String,
    ) : OtaPollEvent()
    public data class Planned(
        val deviceId: String,
        val kind: OtaKind,
        val release: String,
        val daemonVersion: String,
        val imageVersion: String,
        val steps: List<OtaPlanStep>,
    ) : OtaPollEvent()
    public data class Progress(
        val deviceId: String,
        val kind: OtaKind,
        val stepId: Int,
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

public class OtaService(
    private val httpClient: OkHttpClient = OkHttpClient(),
    private val json: Json = defaultJson,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    internal val transferAcks = TransferAckWindow()
    private val mutex = Mutex()
    private val deviceMetaMutex = Mutex()

    private val localZcks = mutableMapOf<String, File>()
    private var rangeServerJob: Job? = null
    private var metaJob: Job? = null
    private var nicknameJob: Job? = null
    private var pollJob: Job? = null

    private var attachedGateway: BridgethingGateway? = null
    private var pollConfig: OtaPollConfig? = null
    private val deviceMeta = mutableMapOf<String, BridgeThingMeta>()
    private val inFlight = mutableSetOf<String>()

    private val imageInstallTargets = mutableMapOf<String, String>()
    private val autoPushNextAt = mutableMapOf<String, Long>()
    private val autoPushFailures = mutableMapOf<String, Int>()
    private val linkOpenAt = mutableMapOf<String, Long>()
    private var pollWake: CompletableDeferred<Unit>? = null


    private val eventsFlow = MutableSharedFlow<OtaPollEvent>(
        extraBufferCapacity = 256,
        onBufferOverflow = BufferOverflow.SUSPEND,
    )

    public val events: Flow<OtaPollEvent> = eventsFlow.asSharedFlow()

    private val storeChangesFlow = MutableSharedFlow<OtaStoreChange>(
        extraBufferCapacity = 256,
        onBufferOverflow = BufferOverflow.SUSPEND,
    )

    public val storeChanges: Flow<OtaStoreChange> = storeChangesFlow.asSharedFlow()

    private val runStore = OtaRunStore()

    private suspend fun emit(event: OtaPollEvent) {
        for (change in runStore.ingest(event, System.currentTimeMillis())) storeChangesFlow.emit(change)
        eventsFlow.emit(event)
    }

    public fun retainedRuns(): List<OtaRun> = runStore.runs()

    public fun retainedAvailable(): List<OtaAvailable> = runStore.available()

    public fun retainedPollStatus(): OtaPollStatus = runStore.pollStatus()

    public suspend fun dismissRun(deviceId: String) {
        val cleared = runStore.dismiss(deviceId) ?: return
        storeChangesFlow.emit(OtaStoreChange.Run(cleared))
    }

    public fun noteRunMeta(deviceId: String, daemonVersion: String, imageVersion: String): OtaRun? =
        runStore.noteMeta(deviceId, daemonVersion, imageVersion)

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
                        is GatewayEvent.Connected -> {
                            noteLinkOpen(event.device.id)
                            wakePoll()
                        }
                        is GatewayEvent.Disconnected -> noteLinkClosed(event.deviceId)
                        is GatewayEvent.LinkFailed -> noteLinkClosed(event.device.id)
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

    public fun setLocalZcks(map: Map<String, File>) {
        localZcks.clear()
        localZcks.putAll(map)
    }

    public fun currentLocalZcks(): Map<String, File> = localZcks.toMap()

    public suspend fun pushUpdate(
        gateway: BridgethingGateway,
        deviceId: String,
        swuPath: File,
        zcks: Map<String, File>,
        updateUrlBase: String? = null,
    ): Flow<OtaPhaseSnapshot> {
        setLocalZcks(zcks)
        return runOtaFlow { collector ->
            val (terminal, _) = driveOta(
                gateway = gateway,
                deviceId = deviceId,
                kind = OtaKind.Image,
                artifactPath = swuPath,
                label = IMAGE_SWU_ASSET,
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
            val terminal = applyBandaidBatch(
                gateway, deviceId,
                listOf(BandaidArtifact(OtaKind.Daemon, binaryPath, "daemon", patch = null)), collector,
            )
            collector(terminal)
        }
    }

    public suspend fun pushBuiltinWebapp(
        gateway: BridgethingGateway,
        deviceId: String,
        bundlePath: File,
    ): Flow<OtaPhaseSnapshot> {
        return runOtaFlow { collector ->
            val terminal = applyBandaidBatch(
                gateway, deviceId,
                listOf(BandaidArtifact(OtaKind.BuiltinWebapp, bundlePath, "webapp", patch = null)), collector,
            )
            collector(terminal)
        }
    }

    public suspend fun pushBandaidBatch(
        gateway: BridgethingGateway,
        deviceId: String,
        artifacts: List<Triple<OtaKind, File, String>>,
    ): Flow<OtaPhaseSnapshot> {
        return runOtaFlow { collector ->
            val terminal = applyBandaidBatch(
                gateway, deviceId,
                artifacts.map { BandaidArtifact(it.first, it.second, it.third, patch = null) }, collector,
            )
            collector(terminal)
        }
    }

    public suspend fun installWebappFromUrl(
        gateway: BridgethingGateway,
        deviceId: String,
        url: String,
        sha256: String,
        size: Long,
        provenance: String?,
        cacheDir: File,
        webappId: String? = null,
        webappName: String? = null,
    ): WebappInstallResult {
        if (!tryBeginInFlight(deviceId)) return WebappInstallResult.Failed(IN_FLIGHT_REASON)
        try {
            val label = webappName ?: "app"
            emit(
                OtaPollEvent.Planned(
                    deviceId = deviceId,
                    kind = OtaKind.InstalledWebapp,
                    release = "",
                    daemonVersion = "",
                    imageVersion = "",
                    steps = listOf(
                        OtaPlanStep(0, OtaStepKind.DOWNLOAD, label, size),
                        OtaPlanStep(1, OtaStepKind.STREAM, label, size),
                        OtaPlanStep(2, OtaStepKind.APPLY, "installing", 0L),
                    ),
                )
            )
            if (webappId != null || webappName != null) {
                runStore.annotateWebapp(deviceId, webappId, webappName)?.let {
                    storeChangesFlow.emit(OtaStoreChange.Run(it))
                }
            }

            val onProgress: suspend (OtaPhaseSnapshot) -> Unit = { snapshot ->
                val step = if (snapshot is OtaPhaseSnapshot.Downloading) 0 else 1
                emit(OtaPollEvent.Progress(deviceId, OtaKind.InstalledWebapp, step, snapshot))
            }

            val bundle = try {
                val expected = OtaArtifactDigest(size = size, sha256 = sha256)
                downloadIfNeeded(
                    url,
                    File(cacheDir, "bridgething-webapp-bundles"),
                    "webapp",
                    url.substringAfterLast('/'),
                    expected,
                    onProgress,
                )
            } catch (e: Throwable) {
                val reason = "bundle download failed: ${e.message ?: e.toString()}"
                emit(OtaPollEvent.Failed(deviceId, OtaKind.InstalledWebapp, reason))
                return WebappInstallResult.Failed(reason)
            }

            val result = performWebappInstall(gateway, deviceId, bundle, provenance, onProgress)
            runCatching { bundle.delete() }

            when (result) {
                is WebappInstallResult.Installed -> {
                    emit(
                        OtaPollEvent.Progress(
                            deviceId, OtaKind.InstalledWebapp, 2,
                            OtaPhaseSnapshot.Applying(OtaPhase.Writing, 100, 100, 0L),
                        )
                    )
                    emit(OtaPollEvent.Updated(deviceId, OtaKind.InstalledWebapp, result.info.version))
                }
                is WebappInstallResult.Failed ->
                    emit(OtaPollEvent.Failed(deviceId, OtaKind.InstalledWebapp, result.reason))
            }
            return result
        } finally {
            endInFlight(deviceId)
        }
    }

    public suspend fun installWebapp(
        gateway: BridgethingGateway,
        deviceId: String,
        bundlePath: File,
        provenance: String?,
        onProgress: (suspend (OtaPhaseSnapshot) -> Unit)? = null,
    ): WebappInstallResult {
        if (!tryBeginInFlight(deviceId)) return WebappInstallResult.Failed(IN_FLIGHT_REASON)
        return try {
            performWebappInstall(gateway, deviceId, bundlePath, provenance, onProgress)
        } finally {
            endInFlight(deviceId)
        }
    }

    private suspend fun performWebappInstall(
        gateway: BridgethingGateway,
        deviceId: String,
        bundlePath: File,
        provenance: String?,
        onProgress: (suspend (OtaPhaseSnapshot) -> Unit)? = null,
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
            patch = null,
            provenance = provenance,
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
                label = "webapp",
                startOffset = resumeFrom.toLong(),
                totalSize = totalSize,
                emit = onProgress,
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

    public suspend fun checkNow(rootUrl: String) {
        val gw = mutex.withLock { attachedGateway } ?: return
        poll(OtaPollConfig(rootUrl = rootUrl, autoPush = false), gw)
    }

    public suspend fun discoverManifest(rootUrl: String): OtaDiscoverManifest =
        fetchManifest("${rootUrl.trimEnd('/')}/manifest.json")

    public suspend fun applyVersion(deviceId: String, channel: String, version: String, rootUrl: String) {
        val gateway = mutex.withLock { attachedGateway } ?: run {
            emit(OtaPollEvent.Failed(deviceId, OtaKind.Image, "gateway not attached"))
            return
        }
        val composite = OtaCompositeVersion.parse(version) ?: run {
            emit(OtaPollEvent.Failed(deviceId, OtaKind.Image, "'$version' is not a composite version"))
            return
        }
        val meta = deviceMetaMutex.withLock { deviceMeta[deviceId] } ?: run {
            emit(OtaPollEvent.Failed(deviceId, OtaKind.Image, "device meta not yet known"))
            return
        }
        if (mutex.withLock { deviceId in inFlight }) return
        val config = OtaPollConfig(rootUrl = rootUrl)
        val urls = OtaArtifactUrls.build(
            rootUrl = rootUrl,
            channel = channel,
            daemonVersion = composite.daemon,
            imageVersion = composite.image,
            imageVariant = meta.imageVariant,
        )
        val artifacts = runCatching { discoverManifest(rootUrl) }.getOrNull()?.releases?.get(version)?.artifacts
        if (meta.imageVersion != composite.image) {
            runImageAuto(
                deviceId, composite.image, version, composite.daemon, channel,
                urls.imageSwu, urls.imageZck, urls.imageBootZck, artifacts, config, gateway,
            )
            return
        }
        if (meta.appVersion != composite.daemon) {
            runBandaidBatchAuto(
                deviceId,
                listOf(
                    daemonPiece(
                        urls = urls, rootUrl = rootUrl, channel = channel,
                        toVersion = composite.daemon, fromVersion = meta.appVersion,
                        fromSha256 = meta.daemonSha256, artifacts = artifacts,
                    ),
                ),
                version,
                composite.daemon,
                composite.image,
                config,
                gateway,
            )
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
        val openedAts = mutex.withLock { linkOpenAt.values.toList() }
        for (openedAt in openedAts) {
            val ready = openedAt + LINK_STABILITY_MS
            if (ready > now && ready < deadline) deadline = ready
        }
        val sleepMs = (deadline - now).coerceAtLeast(0L)
        withTimeoutOrNull(sleepMs) { wake.await() }
        mutex.withLock { pollWake = null }
    }

    private suspend fun wakePoll() {
        mutex.withLock { pollWake }?.complete(Unit)
    }

    private suspend fun noteLinkOpen(deviceId: String) = mutex.withLock {
        linkOpenAt[deviceId] = System.currentTimeMillis()
    }

    private suspend fun noteLinkClosed(deviceId: String) {
        mutex.withLock { linkOpenAt.remove(deviceId) }
        runStore.interrupt(deviceId)?.let { storeChangesFlow.emit(OtaStoreChange.Run(it)) }
    }

    private suspend fun linkStable(deviceId: String): Boolean = mutex.withLock {
        val openedAt = linkOpenAt[deviceId] ?: return@withLock false
        System.currentTimeMillis() - openedAt >= LINK_STABILITY_MS
    }

    private suspend fun recordMeta(deviceId: String, meta: BridgeThingMeta) {
        val isNew = deviceMetaMutex.withLock {
            val fresh = deviceMeta[deviceId] == null
            deviceMeta[deviceId] = meta
            fresh
        }
        metaChangedFlow.emit(deviceId to meta)
        val target = mutex.withLock {
            if (imageInstallTargets[deviceId] == meta.imageVersion) imageInstallTargets.remove(deviceId) else null
        }
        if (target != null) {
            emit(OtaPollEvent.Updated(deviceId = deviceId, kind = OtaKind.Image, version = target))
            noteAutoPushResult(deviceId, failed = false)
        }
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
            emit(OtaPollEvent.ManifestPollFailed(reason = e.message ?: e.toString()))
            return
        }
        emit(OtaPollEvent.ManifestPolled(updatedAt = manifest.updatedAt))

        val snapshot = deviceMetaMutex.withLock { deviceMeta.toMap() }
        for ((deviceId, meta) in snapshot) {
            val channel = manifest.channels[meta.channel] ?: continue
            val composite = OtaCompositeVersion.parse(channel.latest) ?: continue
            val release = manifest.releases[channel.latest]
            if (release != null && (release.yanked != null || release.deprecated)) continue
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
        if (mutex.withLock { deviceId in inFlight }) return

        val channel = meta.channel
        val urls = OtaArtifactUrls.build(
            rootUrl = config.rootUrl,
            channel = channel,
            daemonVersion = latest.daemon,
            imageVersion = latest.image,
            imageVariant = meta.imageVariant,
        )

        val webappDrift = builtinWebappDrift(deviceId, release, channel, config, gateway)
        val imageDrift = meta.imageVersion != latest.image
        val daemonDrift = meta.appVersion != latest.daemon
        if (!imageDrift && !daemonDrift && webappDrift.isEmpty()) return
        emit(
            OtaPollEvent.UpdateAvailable(
                deviceId = deviceId,
                release = latest.composite,
                daemonVersion = latest.daemon,
                imageVersion = latest.image,
            )
        )

        if (imageDrift) {
            if (config.autoPush && autoPushReady(deviceId)) {
                runImageAuto(
                    deviceId, latest.image, latest.composite, latest.daemon, channel,
                    urls.imageSwu, urls.imageZck, urls.imageBootZck, release?.artifacts, config, gateway,
                )
            }
            return
        }

        val batch = mutableListOf<BandaidPiece>()
        if (daemonDrift) {
            batch.add(
                daemonPiece(
                    urls = urls, rootUrl = config.rootUrl, channel = channel,
                    toVersion = latest.daemon, fromVersion = meta.appVersion,
                    fromSha256 = meta.daemonSha256, artifacts = release?.artifacts,
                ),
            )
        }
        for (drift in webappDrift) {
            batch.add(drift.piece)
        }

        if (batch.isNotEmpty() && config.autoPush && autoPushReady(deviceId)) {
            runBandaidBatchAuto(deviceId, batch, latest.composite, latest.daemon, latest.image, config, gateway)
        }
    }

    private data class DaemonPatchPlan(
        val url: String,
        val digest: OtaArtifactDigest,
        val sourceSha256: String?,
        val resultSha256: String,
        val resultSize: Long,
        val algorithm: OtaPatchAlgorithm = OtaPatchAlgorithm.ZstdPatchFrom,
    )

    private data class BandaidPiece(
        val kind: OtaKind,
        val url: String,
        val filename: String,
        val version: String,
        val assetLabel: String,
        val expected: OtaArtifactDigest?,
        val patch: DaemonPatchPlan? = null,
    )

    private data class BandaidArtifact(
        val kind: OtaKind,
        val path: File,
        val label: String,
        val patch: OtaPatch?,
    )

    private fun daemonPiece(
        urls: OtaArtifactUrls,
        rootUrl: String,
        channel: String,
        toVersion: String,
        fromVersion: String,
        fromSha256: String?,
        artifacts: OtaReleaseArtifacts?,
    ): BandaidPiece {
        val daemon = artifacts?.daemon
        val patchDigest = artifacts?.daemonPatches?.get(fromVersion)
        val plan =
            if (daemon != null && patchDigest != null &&
                patchSourceMatches(patchDigest.sourceSha256, fromSha256)
            ) {
                DaemonPatchPlan(
                    url = OtaArtifactUrls.daemonPatch(rootUrl, channel, toVersion, fromVersion),
                    digest = patchDigest.digest,
                    sourceSha256 = patchDigest.sourceSha256,
                    resultSha256 = daemon.sha256,
                    resultSize = daemon.size,
                )
            } else {
                null
            }
        val zst = artifacts?.daemonZst
        if (plan == null && daemon != null && zst != null) {
            return BandaidPiece(
                kind = OtaKind.Daemon,
                url = urls.daemonBinaryZst,
                filename = "daemon-$channel-$toVersion.zst",
                version = toVersion,
                assetLabel = "daemon",
                expected = zst,
                patch = DaemonPatchPlan(
                    url = urls.daemonBinaryZst,
                    digest = zst,
                    sourceSha256 = null,
                    resultSha256 = daemon.sha256,
                    resultSize = daemon.size,
                    algorithm = OtaPatchAlgorithm.Zstd,
                ),
            )
        }
        return BandaidPiece(
            kind = OtaKind.Daemon,
            url = urls.daemonBinary,
            filename = "daemon-$channel-$toVersion",
            version = toVersion,
            assetLabel = "daemon",
            expected = daemon,
            patch = plan,
        )
    }

    private data class WebappDrift(val piece: BandaidPiece, val fromVersion: String)

    private suspend fun builtinWebappDrift(
        deviceId: String,
        release: OtaManifestRelease?,
        channel: String,
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
            val url = OtaArtifactUrls.builtinWebapp(config.rootUrl, channel, slug, available)
            out.add(
                WebappDrift(
                    piece = BandaidPiece(
                        kind = OtaKind.BuiltinWebapp,
                        url = url,
                        filename = "webapp-$channel-$slug-$available",
                        version = available,
                        assetLabel = "webapp: $slug",
                        expected = release.artifacts?.webapps?.get(slug),
                    ),
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

    private suspend fun endInFlight(deviceId: String) = withContext(NonCancellable) {
        mutex.withLock { inFlight.remove(deviceId) }
        val kind = runStore.openRunKind(deviceId) ?: return@withContext
        emit(OtaPollEvent.Failed(deviceId, kind, ABANDONED_REASON))
    }

    private suspend fun autoPushReady(deviceId: String): Boolean {
        if (!linkStable(deviceId)) return false
        return mutex.withLock { System.currentTimeMillis() >= (autoPushNextAt[deviceId] ?: 0L) }
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

    private fun imagePlan(artifacts: OtaReleaseArtifacts?): List<OtaPlanStep> {
        val swu = artifacts?.imageSwu?.size ?: 0L
        val zck = artifacts?.imageZck?.size ?: 0L
        val boot = artifacts?.imageBootZck?.size ?: 0L
        return listOf(
            OtaPlanStep(0, OtaStepKind.DOWNLOAD, IMAGE_SWU_ASSET, swu),
            OtaPlanStep(1, OtaStepKind.DOWNLOAD, SYSTEM_ZCK_ASSET, zck),
            OtaPlanStep(2, OtaStepKind.DOWNLOAD, BOOT_ZCK_ASSET, boot),
            OtaPlanStep(3, OtaStepKind.STREAM, IMAGE_SWU_ASSET, swu),
            OtaPlanStep(4, OtaStepKind.APPLY, "installing image", zck),
            OtaPlanStep(5, OtaStepKind.REBOOT, "reboot", 0L),
        )
    }

    private fun bandaidPlan(pieces: List<BandaidPiece>): List<OtaPlanStep> {
        val steps = mutableListOf<OtaPlanStep>()
        var id = 0
        for (piece in pieces) {
            steps.add(OtaPlanStep(id++, OtaStepKind.DOWNLOAD, piece.assetLabel, piece.expected?.size ?: 0L))
        }
        for (piece in pieces) {
            steps.add(OtaPlanStep(id++, OtaStepKind.STREAM, piece.assetLabel, piece.expected?.size ?: 0L))
        }
        steps.add(OtaPlanStep(id++, OtaStepKind.APPLY, "installing", 0L))
        steps.add(OtaPlanStep(id, OtaStepKind.REBOOT, "reboot", 0L))
        return steps
    }

    private fun routeStep(plan: List<OtaPlanStep>, cursor: Int, snapshot: OtaPhaseSnapshot): Int {
        val fallback = cursor.coerceIn(0, (plan.size - 1).coerceAtLeast(0))
        val match: (OtaPlanStep) -> Boolean = when (snapshot) {
            is OtaPhaseSnapshot.Downloading -> { s -> s.kind == OtaStepKind.DOWNLOAD && s.label == snapshot.asset }
            is OtaPhaseSnapshot.Streaming -> { s -> s.kind == OtaStepKind.STREAM && s.label == snapshot.asset }
            is OtaPhaseSnapshot.Applying -> {
                val want = if (snapshot.phase == OtaPhase.Reboot) OtaStepKind.REBOOT else OtaStepKind.APPLY
                ({ s -> s.kind == want })
            }
            else -> return fallback
        }
        for (i in cursor until plan.size) if (match(plan[i])) return i
        return fallback
    }

    private suspend fun runBandaidBatchAuto(
        deviceId: String,
        pieces: List<BandaidPiece>,
        release: String,
        daemonVersion: String,
        imageVersion: String,
        config: OtaPollConfig,
        gateway: BridgethingGateway,
    ) {
        if (pieces.isEmpty()) return
        if (!tryBeginInFlight(deviceId)) return
        try {
            val cacheDir = effectiveCacheDir(config)
            val labelKind = if (pieces.any { it.kind == OtaKind.Daemon }) OtaKind.Daemon else OtaKind.BuiltinWebapp
            val plan = bandaidPlan(pieces)
            emit(OtaPollEvent.Planned(deviceId, labelKind, release, daemonVersion, imageVersion, plan))

            suspend fun attempt(usePatch: Boolean): OtaPhaseSnapshot {
                var last: OtaPhaseSnapshot = OtaPhaseSnapshot.Idle
                var cursor = 0
                val emit: suspend (OtaPhaseSnapshot) -> Unit = { snapshot ->
                    last = snapshot
                    cursor = routeStep(plan, cursor, snapshot)
                    emit(OtaPollEvent.Progress(deviceId = deviceId, kind = labelKind, stepId = plan[cursor].id, snapshot = snapshot))
                }
                val artifacts = mutableListOf<BandaidArtifact>()
                for (piece in pieces) {
                    val artifact = try {
                        val pplan = piece.patch
                        if (usePatch && pplan != null) {
                            val cached = downloadIfNeeded(pplan.url, cacheDir, "${piece.filename}.patch", piece.assetLabel, pplan.digest, emit)
                            BandaidArtifact(
                                piece.kind, cached, piece.assetLabel,
                                patch = OtaPatch(
                                    algorithm = pplan.algorithm,
                                    resultSha256 = pplan.resultSha256,
                                    resultSize = pplan.resultSize.toUInt(),
                                    sourceSha256 = pplan.sourceSha256,
                                ),
                            )
                        } else {
                            val cached = downloadIfNeeded(piece.url, cacheDir, piece.filename, piece.assetLabel, piece.expected, emit)
                            BandaidArtifact(piece.kind, cached, piece.assetLabel, patch = null)
                        }
                    } catch (e: Throwable) {
                        val reason = "bandaid download failed: ${e.message ?: e.toString()}"
                        emit(OtaPhaseSnapshot.Failed(reason = reason))
                        return OtaPhaseSnapshot.Failed(reason = reason)
                    }
                    artifacts.add(artifact)
                }
                val terminal = applyBandaidBatch(gateway = gateway, deviceId = deviceId, artifacts = artifacts, emit = emit)
                return terminal.takeUnless { it is OtaPhaseSnapshot.Idle } ?: last
            }

            var finalSnap = attempt(usePatch = true)
            if (finalSnap is OtaPhaseSnapshot.Failed && pieces.any { it.patch != null }) {
                finalSnap = attempt(usePatch = false)
            }
            if (finalSnap is OtaPhaseSnapshot.Failed) {
                emit(OtaPollEvent.Failed(deviceId = deviceId, kind = labelKind, reason = finalSnap.reason))
                noteAutoPushResult(deviceId, failed = true)
            } else {
                for (piece in pieces) {
                    emit(OtaPollEvent.Updated(deviceId = deviceId, kind = piece.kind, version = piece.version))
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
        release: String,
        daemonVersion: String,
        channel: String,
        swuUrl: String,
        zckUrl: String,
        bootZckUrl: String,
        artifacts: OtaReleaseArtifacts?,
        config: OtaPollConfig,
        gateway: BridgethingGateway,
    ) {
        if (!tryBeginInFlight(deviceId)) return
        mutex.withLock { imageInstallTargets[deviceId] = targetVersion }
        try {
            val cacheDir = effectiveCacheDir(config)
            val plan = imagePlan(artifacts)
            emit(OtaPollEvent.Planned(deviceId, OtaKind.Image, release, daemonVersion, targetVersion, plan))
            var last: OtaPhaseSnapshot = OtaPhaseSnapshot.Idle
            var cursor = 0
            val emit: suspend (OtaPhaseSnapshot) -> Unit = { snapshot ->
                last = snapshot
                cursor = routeStep(plan, cursor, snapshot)
                emit(OtaPollEvent.Progress(deviceId = deviceId, kind = OtaKind.Image, stepId = plan[cursor].id, snapshot = snapshot))
            }
            val swuLocal: File
            val zckLocal: File
            val bootZckLocal: File
            try {
                swuLocal = downloadIfNeeded(
                    swuUrl, cacheDir, "image-$channel-$targetVersion.swu",
                    IMAGE_SWU_ASSET, artifacts?.imageSwu, emit,
                )
                zckLocal = downloadIfNeeded(
                    zckUrl, cacheDir, "image-$channel-$targetVersion.zck",
                    SYSTEM_ZCK_ASSET, artifacts?.imageZck, emit,
                )
                bootZckLocal = downloadIfNeeded(
                    bootZckUrl, cacheDir, "image-$channel-$targetVersion-boot.zck",
                    BOOT_ZCK_ASSET, artifacts?.imageBootZck, emit,
                )
            } catch (e: Throwable) {
                val reason = "image download failed: ${e.message ?: e.toString()}"
                emit(OtaPhaseSnapshot.Failed(reason = reason))
                emit(OtaPollEvent.Failed(deviceId = deviceId, kind = OtaKind.Image, reason = reason))
                noteAutoPushResult(deviceId, failed = true)
                return
            }
            setLocalZcks(mapOf(SYSTEM_ZCK_ASSET to zckLocal, BOOT_ZCK_ASSET to bootZckLocal))
            val (terminal, _) = driveOta(
                gateway = gateway,
                deviceId = deviceId,
                kind = OtaKind.Image,
                artifactPath = swuLocal,
                label = IMAGE_SWU_ASSET,
                updateUrlBase = config.rootUrl,
                mode = DriveMode.Full,
                emit = emit,
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
        if (kind == OtaKind.Image) {
            val claimed = mutex.withLock { imageInstallTargets.remove(deviceId) != null }
            if (!claimed) return
        }
        when (terminal) {
            is OtaPhaseSnapshot.Completed, is OtaPhaseSnapshot.Staged ->
                emit(OtaPollEvent.Updated(deviceId = deviceId, kind = kind, version = version))
            is OtaPhaseSnapshot.Failed -> emit(
                OtaPollEvent.Failed(deviceId = deviceId, kind = kind, reason = terminal.reason)
            )
            else -> emit(OtaPollEvent.Failed(
                deviceId = deviceId, kind = kind,
                reason = "update ended before completing (last phase: $terminal)",
            ))
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

    private suspend fun downloadIfNeeded(
        url: String,
        dir: File,
        filename: String,
        asset: String,
        expected: OtaArtifactDigest?,
        emit: (suspend (OtaPhaseSnapshot) -> Unit)?,
    ): File = withContext(Dispatchers.IO) {
        if (!dir.exists()) dir.mkdirs()
        val cacheName = expected?.let { "$filename-${it.sha256}" } ?: filename
        val target = File(dir, cacheName)
        if (target.exists()) {
            val size = target.length()
            val reusable = if (expected != null) size == expected.size else size > 0L
            if (reusable) return@withContext target
            target.delete()
        }

        val tracker = RateTracker()
        emit?.invoke(OtaPhaseSnapshot.Downloading(asset = asset, received = 0L, total = expected?.size ?: 0L, ratePerSec = null))
        val tmp = File(dir, "$cacheName.download")
        try {
            val req = Request.Builder().url(url).build()
            httpClient.newCall(req).execute().use { resp ->
                if (!resp.isSuccessful) throw IOException("artifact fetch returned HTTP ${resp.code}")
                val body = resp.body ?: throw IOException("artifact fetch returned empty body")
                val total = body.contentLength().coerceAtLeast(0L)
                body.byteStream().use { input ->
                    tmp.outputStream().use { out ->
                        val buffer = ByteArray(64 * 1024)
                        var received = 0L
                        while (true) {
                            val read = input.read(buffer)
                            if (read == -1) break
                            out.write(buffer, 0, read)
                            received += read
                            tracker.record(received)
                            emit?.invoke(OtaPhaseSnapshot.Downloading(asset, received, total, tracker.ratePerSec()))
                        }
                    }
                }
            }
            if (expected != null) {
                if (tmp.length() != expected.size) throw DigestMismatchException(asset, "size")
                if (hashFile(tmp) != expected.sha256) throw DigestMismatchException(asset, "sha256")
            }
            if (target.exists()) target.delete()
            if (!tmp.renameTo(target)) throw IOException("failed to move downloaded artifact into cache")
            target
        } catch (e: Throwable) {
            tmp.delete()
            throw e
        }
    }

    private suspend fun handleRangeRequest(
        gateway: BridgethingGateway,
        handle: OtaAssetRangeHandle,
        req: OtaAssetRange,
    ) {
        val zck = localZcks[req.asset]
        if (zck == null) {
            runCatching {
                handle.respondErr(OtaAssetRangeRejected(reason = "companion has no cached .zck for asset ${req.asset}"))
            }
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

            var streamOffset: UInt = 0u
            val pacer = TransferPacer()
            try {
                for (part in parts) {
                    raf.seek(part.start.toLong())
                    var produced: UInt = 0u
                    while (produced < part.length) {
                        pacer.observe(transferAcks.receivedBytes(handle.requestId))
                        transferAcks.awaitWindow(
                            handle.requestId, streamOffset.toLong(), pacer.windowBytes, OTA_RANGE_ACK_TIMEOUT_MS,
                        )
                        val want = minOf(pacer.fragmentBytes.toLong(), (part.length - produced).toLong()).toInt()
                        val buf = ByteArray(want)
                        val read = raf.read(buf)
                        if (read <= 0) throw IOException("EOF at ${part.start + produced} before range end")
                        val data = if (read == buf.size) buf else buf.copyOf(read)
                        produced = (produced.toLong() + read.toLong())
                            .coerceAtMost(UInt.MAX_VALUE.toLong()).toUInt()
                        gateway.device(handle.deviceId).transfer.fragment(
                            TransferFragment(transferId = handle.requestId, offset = streamOffset, bytes = data),
                            priority = Priority.Background,
                        )
                        streamOffset += read.toUInt()
                    }
                }
            } catch (e: Throwable) {
                if (e is CancellationException) throw e
                runCatching {
                    gateway.device(handle.deviceId).transfer.abandon(
                        TransferAbandon(transferId = handle.requestId, reason = "range stream failed: ${e.message ?: e.toString()}"),
                    )
                }
                transferAcks.finish(handle.requestId)
                return
            }
            transferAcks.finish(handle.requestId)
        } finally {
            runCatching { raf.close() }
        }
    }

    private enum class DriveMode {
        Full,
        Stage,
    }

    private suspend fun driveOta(
        gateway: BridgethingGateway,
        deviceId: String,
        kind: OtaKind,
        artifactPath: File,
        label: String,
        updateUrlBase: String?,
        mode: DriveMode,
        patch: OtaPatch? = null,
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
            patch = patch,
            provenance = null,
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

        return run {
            emit(OtaPhaseSnapshot.Streaming(asset = label, sent = resumeFrom.toLong(), total = totalSize, ratePerSec = null, etaSeconds = null))

            val success: OtaPhaseSnapshot = if (mode == DriveMode.Full) OtaPhaseSnapshot.Completed else OtaPhaseSnapshot.Staged
            val terminal = CompletableDeferred<OtaPhaseSnapshot>()
            val progressJob = scope.launch {
                gateway.device(deviceId).system.otaProgress.collect { p ->
                    emit(OtaPhaseSnapshot.Applying(
                        phase = p.phase,
                        writePercent = p.percent.toInt().coerceAtMost(100),
                        dwlPercent = p.dwlPercent.toInt().coerceAtMost(100),
                        dwlBytes = p.dwlBytes.toLong(),
                    ))
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
                        label = label,
                        startOffset = resumeFrom.toLong(),
                        totalSize = totalSize,
                        emit = emit,
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
            result to sha
        }
    }

    private suspend fun applyBandaidBatch(
        gateway: BridgethingGateway,
        deviceId: String,
        artifacts: List<BandaidArtifact>,
        emit: suspend (OtaPhaseSnapshot) -> Unit,
    ): OtaPhaseSnapshot {
        val stagedIds = mutableListOf<String>()
        for (artifact in artifacts) {
            val (snapshot, updateId) = driveOta(
                gateway = gateway,
                deviceId = deviceId,
                kind = artifact.kind,
                artifactPath = artifact.path,
                label = artifact.label,
                updateUrlBase = null,
                mode = DriveMode.Stage,
                patch = artifact.patch,
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
                emit(OtaPhaseSnapshot.Applying(
                    phase = p.phase,
                    writePercent = p.percent.toInt().coerceAtMost(100),
                    dwlPercent = p.dwlPercent.toInt().coerceAtMost(100),
                    dwlBytes = p.dwlBytes.toLong(),
                ))
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
        label: String,
        startOffset: Long,
        totalSize: Long,
        emit: (suspend (OtaPhaseSnapshot) -> Unit)?,
    ) = withContext(Dispatchers.IO) {
        val tracker = RateTracker()
        var lastEmitMs = 0L
        suspend fun emitStreaming(sentRaw: Long) {
            val sent = sentRaw.coerceAtMost(totalSize)
            tracker.record(sent)
            val now = System.currentTimeMillis()
            if (now - lastEmitMs < 250L && sent < totalSize) return
            lastEmitMs = now
            val remaining = (totalSize - sent).coerceAtLeast(0L)
            emit?.invoke(
                OtaPhaseSnapshot.Streaming(
                    asset = label, sent = sent, total = totalSize,
                    ratePerSec = tracker.ratePerSec(), etaSeconds = tracker.etaSeconds(remaining),
                ),
            )
        }
        try {
            RandomAccessFile(artifactPath, "r").use { raf ->
                if (startOffset > 0L) {
                    raf.seek(startOffset)
                    transferAcks.note(transferId, startOffset)
                }
                val pacer = TransferPacer(startOffset)
                var offset = startOffset
                while (offset < totalSize) {
                    while (true) {
                        val acked = transferAcks.receivedBytes(transferId)
                        pacer.observe(acked)
                        emitStreaming(acked)
                        if (offset < acked + pacer.windowBytes) break
                        if (!transferAcks.waitForProgress(transferId, acked, OTA_ACK_TIMEOUT_MS)) {
                            throw IOException("transfer stalled: fragment acks stopped at $offset/$totalSize")
                        }
                    }
                    val want = minOf(pacer.fragmentBytes.toLong(), totalSize - offset).toInt()
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
            emitStreaming(totalSize)
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

    private inline fun runOtaFlow(
        crossinline block: suspend (suspend (OtaPhaseSnapshot) -> Unit) -> Unit,
    ): Flow<OtaPhaseSnapshot> = kotlinx.coroutines.flow.channelFlow {
        block { snapshot -> send(snapshot) }
    }

    public fun close() {
        scope.cancel()
    }

    private companion object {
        const val IN_FLIGHT_REASON = "another update is already in flight for this device"
        const val ABANDONED_REASON = "update ended without reporting a result"

        const val IMAGE_SWU_ASSET = "update.swu"
        const val SYSTEM_ZCK_ASSET = "system.img.zck"
        const val BOOT_ZCK_ASSET = "boot.vfat.zck"

        val INLINE_RANGE_MAX_BYTES: UInt = 16u * 1024u

        const val OTA_ACK_TIMEOUT_MS = 15_000L
        const val OTA_RANGE_ACK_TIMEOUT_MS = 30_000L

        const val AUTO_PUSH_BACKOFF_BASE_MS = 120_000L
        const val AUTO_PUSH_BACKOFF_MAX_MS = 15L * 60L * 1000L
        const val MIN_RESUME_DELAY_MS = 5_000L

        const val LINK_STABILITY_MS = 120_000L

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

internal fun patchSourceMatches(declared: String?, running: String?): Boolean {
    if (declared == null || running == null) return true
    return declared.equals(running, ignoreCase = true)
}
