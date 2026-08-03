package com.bridgething.companion

import java.io.File
import java.io.IOException
import java.util.UUID
import java.util.zip.ZipInputStream
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import okhttp3.OkHttpClient

@Serializable
public data class ModelArtifact(
    val url: String,
    val size: Long,
    val sha256: String,
) {
    internal val digest: ArtifactDigest get() = ArtifactDigest(size = size, sha256 = sha256)
}

@Serializable
public data class ModelManifest(
    val version: String,
    @SerialName("updated_at") val updatedAt: String,
    val ios: ModelArtifact? = null,
    val android: ModelArtifact? = null,
)

public sealed class ModelBundleState {
    public object Absent : ModelBundleState()
    public data class Downloading(val received: Long, val total: Long) : ModelBundleState()
    public data class Ready(val version: String) : ModelBundleState()
    public data class Failed(val reason: String) : ModelBundleState()
}

public fun interface ModelTransferPolicy {
    public fun allowsLargeTransfer(): Boolean
}

public class ModelBundleKind private constructor(
    internal val slug: String,
    internal val downloadName: String,
    internal val materialize: (File, File) -> Unit,
    internal val requireShape: (File) -> Unit,
    internal val resolve: (File) -> File,
) {
    public companion object {
        public val Nlu: ModelBundleKind = ModelBundleKind(
            slug = "nlu",
            downloadName = "bundle.zip",
            materialize = { archive, staging -> unzip("nlu", archive, staging) },
            requireShape = { dir -> requireEntries("nlu", dir, listOf("manifest.json", "tokenizer.json", "model.tflite")) },
            resolve = { it },
        )

        public val Asr: ModelBundleKind = ModelBundleKind(
            slug = "asr",
            downloadName = ASR_MODEL_NAME,
            materialize = { file, staging -> file.copyTo(File(staging, ASR_MODEL_NAME), overwrite = true) },
            requireShape = { dir -> requireNonEmpty("asr", File(dir, ASR_MODEL_NAME)) },
            resolve = { File(it, ASR_MODEL_NAME) },
        )
    }
}

internal interface ModelBundleTransport {
    suspend fun manifest(url: String): ModelManifest

    suspend fun download(
        artifact: ModelArtifact,
        dir: File,
        onProgress: suspend (received: Long, total: Long) -> Unit,
    ): File
}

internal class ModelBundleHttpTransport(
    private val fetcher: ArtifactFetcher,
    private val kind: ModelBundleKind,
) : ModelBundleTransport {
    override suspend fun manifest(url: String): ModelManifest =
        fetcher.fetchJson(ModelManifest.serializer(), url)

    override suspend fun download(
        artifact: ModelArtifact,
        dir: File,
        onProgress: suspend (received: Long, total: Long) -> Unit,
    ): File = fetcher.downloadIfNeeded(
        url = artifact.url,
        dir = dir,
        filename = kind.downloadName,
        asset = "${kind.slug} model",
        expected = artifact.digest,
        onProgress = onProgress,
    )
}

internal class ModelBundleException(message: String) : IOException(message)

public class ModelBundleStore internal constructor(
    private val kind: ModelBundleKind,
    private val config: Config,
    enabled: Boolean,
    private val transport: ModelBundleTransport,
    private val transferPolicy: ModelTransferPolicy,
    private val validator: suspend (File) -> Unit,
) {
    public data class Config(
        val storageDirectory: File,
        val rootUrl: String = "https://ota.bridgething.com",
        val channel: String = "stable",
    )

    public constructor(
        kind: ModelBundleKind,
        config: Config,
        enabled: Boolean = true,
        transferPolicy: ModelTransferPolicy = ModelTransferPolicy { true },
        httpClient: OkHttpClient = OkHttpClient(),
        json: Json = defaultJson,
        validator: suspend (File) -> Unit,
    ) : this(
        kind = kind,
        config = config,
        enabled = enabled,
        transport = ModelBundleHttpTransport(ArtifactFetcher(httpClient, json), kind),
        transferPolicy = transferPolicy,
        validator = validator,
    )

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val mutex = Mutex()
    private val root = File(config.storageDirectory, "bridgething-${kind.slug}")

    private var enabled: Boolean = enabled
    private var inFlight: Job? = null

    private val stateFlow: MutableStateFlow<ModelBundleState> = MutableStateFlow(
        readCurrent()?.takeIf { installLooksComplete(File(root, it)) }
            ?.let { ModelBundleState.Ready(it) }
            ?: ModelBundleState.Absent
    )

    public val states: StateFlow<ModelBundleState> = stateFlow.asStateFlow()

    public val state: ModelBundleState get() = stateFlow.value

    public val live: File?
        get() = (stateFlow.value as? ModelBundleState.Ready)?.let { kind.resolve(File(root, it.version)) }

    public suspend fun setEnabled(value: Boolean) {
        val previous = mutex.withLock {
            val was = enabled
            enabled = value
            was
        }
        if (previous == value) return
        if (value) {
            scope.launch { ensure() }
            return
        }
        val running = mutex.withLock { inFlight.also { inFlight = null } }
        running?.cancelAndJoin()
        withContext(NonCancellable + Dispatchers.IO) { root.deleteRecursively() }
        publish(ModelBundleState.Absent)
    }

    public suspend fun ensure() {
        val job = mutex.withLock {
            if (!enabled) return
            inFlight ?: scope.launch { run() }.also { inFlight = it }
        }
        job.join()
        mutex.withLock { if (inFlight === job) inFlight = null }
    }

    public fun close() {
        scope.cancel()
    }

    private suspend fun run() {
        try {
            val manifest = transport.manifest(manifestUrl())
            val artifact = manifest.android
                ?: throw ModelBundleException("the ${kind.slug} manifest carries nothing for this platform")
            val installed = readCurrent()
            if (installed == manifest.version && installLooksComplete(File(root, manifest.version))) {
                publish(ModelBundleState.Ready(manifest.version))
                return
            }
            if (!transferPolicy.allowsLargeTransfer()) {
                publish(installed?.let { ModelBundleState.Ready(it) } ?: ModelBundleState.Absent)
                return
            }
            install(manifest.version, artifact)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Throwable) {
            val installed = readCurrent()
            publish(
                if (installed != null) ModelBundleState.Ready(installed)
                else ModelBundleState.Failed(e.message ?: e.toString())
            )
        }
    }

    private suspend fun install(version: String, artifact: ModelArtifact) {
        withContext(Dispatchers.IO) { root.mkdirs() }
        val downloads = File(root, "downloads")
        val staging = File(root, "staging-${UUID.randomUUID()}")

        publish(ModelBundleState.Downloading(received = 0L, total = artifact.size))
        val downloaded = transport.download(artifact, downloads) { received, reported ->
            publishProgress(received, if (artifact.size > 0L) artifact.size else reported)
        }

        try {
            withContext(Dispatchers.IO) {
                staging.mkdirs()
                kind.materialize(downloaded, staging)
                kind.requireShape(staging)
            }
            validator(kind.resolve(staging))
            withContext(Dispatchers.IO) { rotate(staging, version) }
        } catch (e: Throwable) {
            withContext(NonCancellable + Dispatchers.IO) { staging.deleteRecursively() }
            throw e
        }

        withContext(Dispatchers.IO) { downloads.deleteRecursively() }
        publish(ModelBundleState.Ready(version))
    }

    private fun rotate(staging: File, version: String) {
        val live = File(root, version)
        if (live.exists()) live.deleteRecursively()
        if (!staging.renameTo(live)) throw ModelBundleException("failed to move the staged ${kind.slug} model into place")
        File(root, CURRENT_FILE).writeText(version)
        pruneSuperseded(version)
    }

    private fun pruneSuperseded(version: String) {
        val entries = root.listFiles() ?: return
        for (entry in entries) {
            if (entry.name == version || entry.name == CURRENT_FILE) continue
            entry.deleteRecursively()
        }
    }

    private fun manifestUrl(): String = "${config.rootUrl.trimEnd('/')}/${kind.slug}/${config.channel}/manifest.json"

    private fun readCurrent(): String? =
        runCatching { File(root, CURRENT_FILE).readText().trim() }.getOrNull()?.takeIf { it.isNotEmpty() }

    private fun installLooksComplete(dir: File): Boolean = runCatching { kind.requireShape(dir) }.isSuccess

    private suspend fun publish(next: ModelBundleState) {
        mutex.withLock { stateFlow.value = next }
    }

    private suspend fun publishProgress(received: Long, total: Long) {
        mutex.withLock {
            if (stateFlow.value is ModelBundleState.Downloading) {
                stateFlow.value = ModelBundleState.Downloading(received = received, total = total)
            }
        }
    }

    internal companion object {
        const val CURRENT_FILE = "current"

        val defaultJson: Json = Json {
            ignoreUnknownKeys = true
            isLenient = true
        }
    }
}

private const val ASR_MODEL_NAME = "model.bin"

private fun requireEntries(slug: String, dir: File, entries: List<String>) {
    for (entry in entries) {
        if (!File(dir, entry).exists()) throw ModelBundleException("$slug archive is missing $entry")
    }
}

private fun requireNonEmpty(slug: String, file: File) {
    if (!file.isFile || file.length() == 0L) throw ModelBundleException("$slug model is missing ${file.name}")
}

private fun unzip(slug: String, archive: File, into: File) {
    val rootPath = into.canonicalFile.toPath()
    ZipInputStream(archive.inputStream().buffered()).use { zin ->
        while (true) {
            val entry = zin.nextEntry ?: break
            val out = File(into, entry.name)
            if (!out.canonicalFile.toPath().startsWith(rootPath)) {
                throw ModelBundleException("$slug archive entry escapes the staging root: ${entry.name}")
            }
            if (entry.isDirectory) {
                out.mkdirs()
            } else {
                out.parentFile?.mkdirs()
                out.outputStream().use { zin.copyTo(it) }
            }
            zin.closeEntry()
        }
    }
}
