package dev.bridgething.companion

import dev.bridgething.gateway.BridgethingGateway
import dev.bridgething.gateway.GatewayEvent
import dev.bridgething.gateway.RequestResult
import dev.bridgething.gateway.webapp
import dev.bridgething.schema.BridgeThingMeta
import dev.bridgething.schema.BridgeToGatewayMsgData
import dev.bridgething.schema.WebappInfo
import dev.bridgething.schema.WebappRole
import dev.bridgething.schema.WebappSource
import java.io.File
import java.io.IOException
import java.security.MessageDigest
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

/**
 * Persists the catalog subscription list and the provenance pins (which source
 * each installed app came from). Injected so the host can back it with whatever
 * storage it prefers; the default writes JSON files.
 */
public interface CatalogStore {
    public suspend fun loadSources(): List<String>
    public suspend fun saveSources(urls: List<String>)
    public suspend fun loadPins(): Map<String, String>
    public suspend fun savePins(pins: Map<String, String>)
}

/** Fetches catalogs and downloads bundles. Injected so aggregation and install can be tested offline. */
public interface CatalogFetcher {
    public suspend fun fetchCatalog(url: String): Catalog
    public suspend fun download(url: String, destination: File)
}

/** The byte pump that streams a bundle to the device. [OtaService] implements it; the catalog reuses the one OTA path. */
public interface WebappInstaller {
    public suspend fun installWebapp(gateway: BridgethingGateway, deviceId: String, bundlePath: File): WebappInstallResult
}

/**
 * One aggregated entry in the store UI: an app drawn from its primary source
 * (the pinned source if installed, else the first subscribed source offering
 * it), annotated with compat, install state, and where else it is available.
 */
@Serializable
public data class CatalogAppListing(
    val app: CatalogApp,
    val sourceUrl: String,
    /** Newest version whose min_libbridgething_version the device satisfies, or null if too old for every version. */
    val newestCompatible: CatalogAppVersion?,
    val installedVersion: String?,
    val updateAvailable: Boolean,
    /** Other subscribed sources that list this same uuid. */
    val alsoAvailableFrom: List<String>,
)

/** A pending update for an installed app, sourced only from its pinned source. */
@Serializable
public data class CatalogAppUpdate(
    val appId: String,
    val name: String,
    val installedVersion: String,
    val target: CatalogAppVersion,
    val sourceUrl: String,
)

/**
 * Background update-poll configuration. Off by default; even when on, the
 * default is to surface [CatalogEvent.UpdateAvailable] rather than install
 * silently, since the user curates which webapps live on their device.
 */
public data class CatalogPollConfig(
    val intervalSeconds: Long = 6 * 60 * 60L,
    val autoInstall: Boolean = false,
)

/** High-level events the host app drives UI from. */
public sealed class CatalogEvent {
    public data class Refreshed(val sourceCount: Int, val appCount: Int) : CatalogEvent()
    public data class SourceFailed(val url: String, val reason: String) : CatalogEvent()
    public data class UpdateAvailable(val deviceId: String, val update: CatalogAppUpdate) : CatalogEvent()
    public data class Installed(val deviceId: String, val appId: String, val version: String) : CatalogEvent()
    public data class InstallFailed(val deviceId: String, val appId: String, val reason: String) : CatalogEvent()
}

/**
 * The webapp app-store service. Owns the subscription list and provenance pins,
 * aggregates `catalog.v1` sources, filters by the device's libbridgethingVersion,
 * and drives installs/updates through the OTA byte pump. Delivery is never
 * reimplemented here; only discovery, provenance, and compat live in this class.
 */
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
    private var pins: Map<String, String> = emptyMap()
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

    /** High-level events the host app drives UI off. */
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

    public suspend fun pinnedSource(appId: String): String? = mutex.withLock { loadStateLocked(); pins[appId] }

    // browse

    /** Fetch every subscribed catalog. A source that fails emits [CatalogEvent.SourceFailed] and keeps its last-known catalog. */
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

    /**
     * Aggregated, compat-annotated listings for the store UI. Reads the device's
     * installed apps and its libbridgethingVersion; call [refresh] first.
     */
    public suspend fun availableApps(deviceId: String): List<CatalogAppListing> {
        val installed = installedApps(deviceId)
        val (ordered, currentPins, deviceLib) = mutex.withLock {
            loadStateLocked()
            Triple(orderedCatalogsLocked(), pins.toMap(), deviceMeta[deviceId]?.libbridgethingVersion)
        }
        return aggregate(ordered, installed, currentPins, deviceLib)
    }

    // install

    /**
     * Download the version's zip, verify it matches the catalog's declared size +
     * sha256, deliver it via the OTA pump, and on success pin the app to the
     * source it came from. The sha256 check here is the catalog's integrity
     * guarantee; the daemon separately verifies transfer integrity.
     */
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

        val result = installer.installWebapp(gateway, deviceId, bundle)
        runCatching { bundle.delete() }
        when (result) {
            is WebappInstallResult.Installed -> {
                mutex.withLock {
                    pins = pins + (app.id to sourceUrl)
                    store.savePins(pins)
                }
                eventsFlow.emit(CatalogEvent.Installed(deviceId, app.id, result.info.version))
            }
            is WebappInstallResult.Failed ->
                eventsFlow.emit(CatalogEvent.InstallFailed(deviceId, app.id, result.reason))
        }
        return result
    }

    /**
     * Install by ids, resolving the app + version from the last refreshed catalog
     * for [sourceUrl]. Convenience for callers (the RN bridge) that hold
     * identifiers rather than the decoded objects.
     */
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

    /** Pending updates for installed apps, each sourced only from the app's pinned source. */
    public suspend fun checkForUpdates(deviceId: String): List<CatalogAppUpdate> {
        val installed = installedApps(deviceId)
        val (snapshot, currentPins, deviceLib) = mutex.withLock {
            loadStateLocked()
            Triple(catalogs.toMap(), pins.toMap(), deviceMeta[deviceId]?.libbridgethingVersion)
        }
        return updates(snapshot, currentPins, installed, deviceLib)
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
        pins = store.loadPins()
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
            pins: Map<String, String>,
            deviceLibVersion: String?,
        ): List<CatalogAppListing> {
            val installedById = installed.associateBy { it.id.toString().lowercase() }

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
            pins: Map<String, String>,
            installed: List<WebappInfo>,
            deviceLibVersion: String?,
        ): List<CatalogAppUpdate> {
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

        /**
         * Newest version (catalog order is newest-first) the device satisfies. A
         * null device version means it has not announced yet; treat every version
         * as compatible so the UI can still list the app.
         */
        internal fun newestCompatible(app: CatalogApp, deviceLibVersion: String?): CatalogAppVersion? {
            if (deviceLibVersion == null) return app.versions.firstOrNull()
            return app.versions.firstOrNull {
                SemverCompat.satisfies(deviceLibVersion, it.minLibbridgethingVersion)
            }
        }
    }
}

/** File-backed [CatalogStore]. Stores two small JSON files under [directory]. */
public class FileCatalogStore(private val directory: File) : CatalogStore {
    private val json = Json { prettyPrint = false }
    private val sourcesFile get() = File(directory, "sources.json")
    private val pinsFile get() = File(directory, "pins.json")

    override suspend fun loadSources(): List<String> = withContext(Dispatchers.IO) {
        runCatching { json.decodeFromString<List<String>>(sourcesFile.readText()) }.getOrDefault(emptyList())
    }

    override suspend fun saveSources(urls: List<String>): Unit = withContext(Dispatchers.IO) {
        directory.mkdirs()
        sourcesFile.writeText(json.encodeToString(urls))
    }

    override suspend fun loadPins(): Map<String, String> = withContext(Dispatchers.IO) {
        runCatching { json.decodeFromString<Map<String, String>>(pinsFile.readText()) }.getOrDefault(emptyMap())
    }

    override suspend fun savePins(pins: Map<String, String>): Unit = withContext(Dispatchers.IO) {
        directory.mkdirs()
        pinsFile.writeText(json.encodeToString(pins))
    }
}

/** In-memory [CatalogStore] for tests and hosts that persist elsewhere. */
public class InMemoryCatalogStore(
    sources: List<String> = emptyList(),
    pins: Map<String, String> = emptyMap(),
) : CatalogStore {
    private val mutex = Mutex()
    private var sources: List<String> = sources
    private var pins: Map<String, String> = pins

    override suspend fun loadSources(): List<String> = mutex.withLock { sources }
    override suspend fun saveSources(urls: List<String>) { mutex.withLock { sources = urls } }
    override suspend fun loadPins(): Map<String, String> = mutex.withLock { pins }
    override suspend fun savePins(pins: Map<String, String>) { mutex.withLock { this.pins = pins } }
}

/** Default [CatalogFetcher] over OkHttp. */
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
