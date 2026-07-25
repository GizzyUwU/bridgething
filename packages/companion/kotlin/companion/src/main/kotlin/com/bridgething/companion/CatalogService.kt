package com.bridgething.companion

import com.bridgething.gateway.BridgethingGateway
import com.bridgething.gateway.GatewayEvent
import com.bridgething.gateway.RequestResult
import com.bridgething.gateway.webapp
import com.bridgething.schema.BridgeThingMeta
import com.bridgething.schema.BridgeToGatewayMsgData
import com.bridgething.schema.WebappInfo
import com.bridgething.schema.WebappRole
import com.bridgething.schema.WebappSource
import java.io.File
import java.io.IOException
import java.security.MessageDigest
import java.time.Instant
import java.time.OffsetDateTime
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
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
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import okhttp3.OkHttpClient
import okhttp3.Request

public interface CatalogStore {
    public suspend fun loadSources(): List<String>
    public suspend fun saveSources(urls: List<String>)
}

public interface CatalogFetcher {
    public suspend fun fetchCatalog(url: String): Catalog
    public suspend fun download(url: String, destination: File)
}

public interface WebappInstaller {
    public suspend fun installWebapp(
        gateway: BridgethingGateway,
        deviceId: String,
        bundlePath: File,
        provenance: String?,
    ): WebappInstallResult
}

@Serializable
public data class CatalogAppListing(
    val app: CatalogApp,
    val sourceUrl: String,
    val newestCompatible: CatalogAppVersion?,
    val installedVersion: String?,
    val updateAvailable: Boolean,
    val alsoAvailableFrom: List<String>,
)

@Serializable
public data class CatalogAppUpdate(
    val appId: String,
    val name: String,
    val installedVersion: String,
    val target: CatalogAppVersion,
    val sourceUrl: String,
)

public data class CatalogPollConfig(
    val intervalSeconds: Long = 6 * 60 * 60L,
    val autoInstall: Boolean = false,
)

public sealed class CatalogEvent {
    public data class Refreshed(val sourceCount: Int, val appCount: Int) : CatalogEvent()
    public data class SourceFailed(val url: String, val reason: String) : CatalogEvent()
    public data class UpdateAvailable(val deviceId: String, val update: CatalogAppUpdate) : CatalogEvent()
    public data class Installed(val deviceId: String, val appId: String, val version: String) : CatalogEvent()
    public data class InstallFailed(val deviceId: String, val appId: String, val reason: String) : CatalogEvent()
}

public class CatalogService(
    private val installer: WebappInstaller,
    private val store: CatalogStore = InMemoryCatalogStore(),
    httpClient: OkHttpClient = OkHttpClient(),
    json: Json = defaultJson,
    private val fetcher: CatalogFetcher = OkHttpCatalogFetcher(httpClient, json),
    private val officialCatalogUrl: String = "https://apps.bridgething.com/catalog.json",
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val mutex = Mutex()

    private var attachedGateway: BridgethingGateway? = null
    private var sourceUrls: List<String> = emptyList()
    private val deviceMeta = mutableMapOf<String, BridgeThingMeta>()
    private val catalogs = mutableMapOf<String, Catalog>()
    private var loaded = false

    private var metaJob: Job? = null
    private var pollJob: Job? = null
    private var pollConfig: CatalogPollConfig? = null

    private val eventsFlow = MutableSharedFlow<CatalogEvent>(
        extraBufferCapacity = 64,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )

    public val events: Flow<CatalogEvent> = eventsFlow.asSharedFlow()

    public suspend fun start(gateway: BridgethingGateway) {
        mutex.withLock {
            attachedGateway = gateway
            loadStateLocked()
            metaJob?.cancel()
            metaJob = scope.launch {
                gateway.events.collect { event ->
                    if (event !is GatewayEvent.Message) return@collect
                    val data = event.message.data
                    if (data is BridgeToGatewayMsgData.Version) recordMeta(event.deviceId, data.data)
                }
            }
        }
    }

    public suspend fun stop() {
        mutex.withLock {
            metaJob?.cancel(); metaJob = null
            pollJob?.cancel(); pollJob = null
            attachedGateway = null
            deviceMeta.clear()
        }
    }

    // sources

    public suspend fun sources(): List<String> = mutex.withLock { loadStateLocked(); sourceUrls }

    public suspend fun addSource(url: String) {
        mutex.withLock {
            loadStateLocked()
            if (url in sourceUrls) return@withLock
            sourceUrls = sourceUrls + url
            store.saveSources(sourceUrls)
        }
    }

    public suspend fun removeSource(url: String) {
        mutex.withLock {
            loadStateLocked()
            if (url !in sourceUrls) return@withLock
            sourceUrls = sourceUrls - url
            catalogs.remove(url)
            store.saveSources(sourceUrls)
        }
    }

    public suspend fun pinnedSource(deviceId: String, appId: String): String? =
        pins(installedApps(deviceId))[appId]

    // browse

    public suspend fun refresh() {
        val urls = mutex.withLock { loadStateLocked(); sourceUrls }
        for (url in urls) {
            try {
                val catalog = fetcher.fetchCatalog(url)
                mutex.withLock { catalogs[url] = catalog }
            } catch (e: Throwable) {
                eventsFlow.emit(CatalogEvent.SourceFailed(url, e.message ?: e.toString()))
            }
        }
        val (count, appCount) = mutex.withLock { catalogs.size to catalogs.values.sumOf { it.apps.size } }
        eventsFlow.emit(CatalogEvent.Refreshed(count, appCount))
    }

    public suspend fun availableApps(deviceId: String): List<CatalogAppListing> {
        val installed = installedApps(deviceId)
        val (ordered, deviceLib) = mutex.withLock {
            loadStateLocked()
            orderedCatalogsLocked() to deviceMeta[deviceId]?.libbridgethingVersion
        }
        return aggregate(ordered, installed, deviceLib)
    }

    // install

    public suspend fun install(
        deviceId: String,
        app: CatalogApp,
        version: CatalogAppVersion,
        sourceUrl: String,
    ): WebappInstallResult {
        val gateway = mutex.withLock { attachedGateway } ?: run {
            val reason = "gateway not attached"
            eventsFlow.emit(CatalogEvent.InstallFailed(deviceId, app.id, reason))
            return WebappInstallResult.Failed(reason)
        }
        val bundle = try {
            downloadVerified(version, app.id)
        } catch (e: Throwable) {
            val reason = e.message ?: e.toString()
            eventsFlow.emit(CatalogEvent.InstallFailed(deviceId, app.id, reason))
            return WebappInstallResult.Failed(reason)
        }

        val result = installer.installWebapp(gateway, deviceId, bundle, sourceUrl)
        runCatching { bundle.delete() }
        when (result) {
            is WebappInstallResult.Installed ->
                eventsFlow.emit(CatalogEvent.Installed(deviceId, app.id, result.info.version))
            is WebappInstallResult.Failed ->
                eventsFlow.emit(CatalogEvent.InstallFailed(deviceId, app.id, result.reason))
        }
        return result
    }

    public suspend fun install(deviceId: String, appId: String, version: String, sourceUrl: String): WebappInstallResult {
        val (app, ver) = mutex.withLock {
            loadStateLocked()
            val a = catalogs[sourceUrl]?.apps?.firstOrNull { it.id == appId }
            a to a?.versions?.firstOrNull { it.version == version }
        }
        if (app == null || ver == null) {
            val reason = "app $appId@$version not found in $sourceUrl; refresh first"
            eventsFlow.emit(CatalogEvent.InstallFailed(deviceId, appId, reason))
            return WebappInstallResult.Failed(reason)
        }
        return install(deviceId, app, ver, sourceUrl)
    }

    // updates

    public suspend fun checkForUpdates(deviceId: String): List<CatalogAppUpdate> {
        val installed = installedApps(deviceId)
        val (snapshot, deviceLib) = mutex.withLock {
            loadStateLocked()
            catalogs.toMap() to deviceMeta[deviceId]?.libbridgethingVersion
        }
        return updates(snapshot, installed, deviceLib)
    }

    public suspend fun setPollConfig(config: CatalogPollConfig?) {
        mutex.withLock {
            pollConfig = config
            pollJob?.cancel(); pollJob = null
            if (config != null) pollJob = scope.launch { runPollLoop(config) }
        }
    }

    public suspend fun pollNow() {
        val config = mutex.withLock { pollConfig } ?: return
        pollOnce(config)
    }

    private suspend fun runPollLoop(config: CatalogPollConfig) {
        while (scope.isActive) {
            pollOnce(config)
            delay(config.intervalSeconds.coerceAtLeast(60L) * 1000L)
        }
    }

    private suspend fun pollOnce(config: CatalogPollConfig) {
        refresh()
        val deviceIds = mutex.withLock { deviceMeta.keys.toList() }
        for (deviceId in deviceIds) {
            for (update in checkForUpdates(deviceId)) {
                eventsFlow.emit(CatalogEvent.UpdateAvailable(deviceId, update))
                if (!config.autoInstall) continue
                val app = mutex.withLock { catalogs[update.sourceUrl]?.apps?.firstOrNull { it.id == update.appId } } ?: continue
                install(deviceId, app, update.target, update.sourceUrl)
            }
        }
    }

    // internals

    private fun orderedCatalogsLocked(): List<Pair<String, Catalog>> =
        sourceUrls.mapNotNull { url -> catalogs[url]?.let { url to it } }

    private suspend fun loadStateLocked() {
        if (loaded) return
        var sources = store.loadSources()
        if (sources.isEmpty()) {
            sources = listOf(officialCatalogUrl)
            store.saveSources(sources)
        }
        sourceUrls = sources
        loaded = true
    }

    private fun recordMeta(deviceId: String, meta: BridgeThingMeta) {
        deviceMeta[deviceId] = meta
    }

    private suspend fun installedApps(deviceId: String): List<WebappInfo> {
        val gateway = mutex.withLock { attachedGateway } ?: return emptyList()
        return when (val r = gateway.webapp.list(deviceId)) {
            is RequestResult.Ok -> r.response.webapps
            else -> emptyList()
        }
    }

    private suspend fun downloadVerified(version: CatalogAppVersion, appId: String): File = withContext(Dispatchers.IO) {
        val dir = File(System.getProperty("java.io.tmpdir") ?: "/tmp", "bridgething-catalog").apply { mkdirs() }
        val dest = File(dir, "$appId-${version.version}.zip")
        dest.delete()
        fetcher.download(version.download.url, dest)
        if (dest.length() != version.download.size) {
            dest.delete()
            throw IOException("download size ${dest.length()} != catalog size ${version.download.size}")
        }
        val digest = hashFile(dest)
        if (digest != version.download.sha256.lowercase()) {
            dest.delete()
            throw IOException("download sha256 $digest != catalog sha256 ${version.download.sha256}")
        }
        dest
    }

    private fun hashFile(file: File): String {
        val md = MessageDigest.getInstance("SHA-256")
        file.inputStream().use { input ->
            val buf = ByteArray(64 * 1024)
            while (true) {
                val n = input.read(buf)
                if (n <= 0) break
                md.update(buf, 0, n)
            }
        }
        return md.digest().joinToString("") { "%02x".format(it) }
    }

    public fun close() {
        scope.cancel()
    }

    public companion object {
        internal val defaultJson: Json = Json {
            ignoreUnknownKeys = true
            isLenient = true
        }

        internal fun aggregate(
            orderedCatalogs: List<Pair<String, Catalog>>,
            installed: List<WebappInfo>,
            deviceLibVersion: String?,
        ): List<CatalogAppListing> {
            val installedById = installed.associateBy { it.id.toString().lowercase() }
            val pins = pins(installed)

            val offerings = LinkedHashMap<String, MutableList<Pair<String, CatalogApp>>>()
            for ((url, catalog) in orderedCatalogs) {
                for (app in catalog.apps) {
                    offerings.getOrPut(app.id) { mutableListOf() }.add(url to app)
                }
            }

            val listings = mutableListOf<CatalogAppListing>()
            for ((id, offers) in offerings) {
                if (offers.isEmpty()) continue
                val pinned = pins[id]
                val primary = offers.firstOrNull { it.first == pinned } ?: offers.first()
                val alsoFrom = offers.map { it.first }.filter { it != primary.first }

                val newest = newestCompatible(primary.second, deviceLibVersion)
                val installedVersion = installedById[id]?.version
                val updateAvailable = installedVersion != null &&
                    newest != null &&
                    newest.version != installedVersion

                listings.add(
                    CatalogAppListing(
                        app = primary.second,
                        sourceUrl = primary.first,
                        newestCompatible = newest,
                        installedVersion = installedVersion,
                        updateAvailable = updateAvailable,
                        alsoAvailableFrom = alsoFrom,
                    )
                )
            }
            return listings.sortedWith(compareBy({ it.app.name }, { it.app.id }))
        }

        internal fun updates(
            catalogs: Map<String, Catalog>,
            installed: List<WebappInfo>,
            deviceLibVersion: String?,
        ): List<CatalogAppUpdate> {
            val pins = pins(installed)
            val out = mutableListOf<CatalogAppUpdate>()
            for (info in installed) {
                if (info.source != WebappSource.Installed || info.role != WebappRole.Standard) continue
                val id = info.id.toString().lowercase()
                val sourceUrl = pins[id] ?: continue
                val app = catalogs[sourceUrl]?.apps?.firstOrNull { it.id == id } ?: continue
                val newest = newestCompatible(app, deviceLibVersion) ?: continue
                if (newest.version == info.version) continue
                out.add(
                    CatalogAppUpdate(
                        appId = id,
                        name = app.name,
                        installedVersion = info.version,
                        target = newest,
                        sourceUrl = sourceUrl,
                    )
                )
            }
            return out.sortedWith(compareBy({ it.name }, { it.appId }))
        }

        internal fun pins(installed: List<WebappInfo>): Map<String, String> =
            installed.mapNotNull { info ->
                info.provenance?.let { info.id.toString().lowercase() to it }
            }.toMap()

        internal fun releasedAtInstant(raw: String): Instant? =
            runCatching { OffsetDateTime.parse(raw).toInstant() }.getOrNull()

        internal fun sortedByReleasedAt(versions: List<CatalogAppVersion>): List<CatalogAppVersion> =
            versions.withIndex().sortedWith(
                compareBy<IndexedValue<CatalogAppVersion>> { releasedAtInstant(it.value.releasedAt) == null }
                    .thenByDescending { releasedAtInstant(it.value.releasedAt) ?: Instant.MIN }
                    .thenBy { it.index }
            ).map { it.value }

        internal fun newestCompatible(app: CatalogApp, deviceLibVersion: String?): CatalogAppVersion? {
            val ordered = sortedByReleasedAt(app.versions)
            if (deviceLibVersion == null) return ordered.firstOrNull()
            return ordered.firstOrNull {
                SemverCompat.satisfies(deviceLibVersion, it.minLibbridgethingVersion)
            }
        }
    }
}

public class FileCatalogStore(private val directory: File) : CatalogStore {
    private val json = Json { prettyPrint = false }
    private val sourcesFile get() = File(directory, "sources.json")

    override suspend fun loadSources(): List<String> = withContext(Dispatchers.IO) {
        runCatching { json.decodeFromString<List<String>>(sourcesFile.readText()) }.getOrDefault(emptyList())
    }

    override suspend fun saveSources(urls: List<String>): Unit = withContext(Dispatchers.IO) {
        directory.mkdirs()
        sourcesFile.writeText(json.encodeToString(urls))
    }
}

public class InMemoryCatalogStore(
    sources: List<String> = emptyList(),
) : CatalogStore {
    private val mutex = Mutex()
    private var sources: List<String> = sources

    override suspend fun loadSources(): List<String> = mutex.withLock { sources }
    override suspend fun saveSources(urls: List<String>) { mutex.withLock { sources = urls } }
}

public class OkHttpCatalogFetcher(
    private val httpClient: OkHttpClient,
    private val json: Json,
) : CatalogFetcher {
    override suspend fun fetchCatalog(url: String): Catalog = withContext(Dispatchers.IO) {
        val req = Request.Builder().url(url).build()
        httpClient.newCall(req).execute().use { resp ->
            if (!resp.isSuccessful) throw IOException("catalog fetch returned HTTP ${resp.code}")
            val body = resp.body?.string() ?: throw IOException("catalog fetch returned empty body")
            json.decodeFromString(Catalog.serializer(), body)
        }
    }

    override suspend fun download(url: String, destination: File): Unit = withContext(Dispatchers.IO) {
        val req = Request.Builder().url(url).build()
        httpClient.newCall(req).execute().use { resp ->
            if (!resp.isSuccessful) throw IOException("bundle fetch returned HTTP ${resp.code}")
            val body = resp.body ?: throw IOException("bundle fetch returned empty body")
            destination.outputStream().use { out -> body.byteStream().use { it.copyTo(out) } }
        }
    }
}
