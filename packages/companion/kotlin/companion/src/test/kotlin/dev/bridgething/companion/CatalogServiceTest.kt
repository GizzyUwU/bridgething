package dev.bridgething.companion

import dev.bridgething.gateway.BridgethingGateway
import dev.bridgething.schema.WebappInfo
import dev.bridgething.schema.WebappRole
import dev.bridgething.schema.WebappSource
import java.io.File
import java.io.IOException
import java.util.UUID
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

private const val CALENDAR_ID = "019e6701-13f8-71b5-ba04-85d326630e98"
private const val WEATHER_ID = "019e6701-13f8-71b5-ba04-81f347137de2"
private const val SOURCE_A = "https://apps.bridgething.com/catalog.json"
private const val SOURCE_B = "https://repo.example.com/catalog.json"
private val SHA = "0".repeat(64)

private fun ver(v: String, minLib: String = "12.0.0", released: String = "2026-05-31T00:00:00Z") =
    CatalogAppVersion(
        version = v,
        releasedAt = released,
        download = CatalogDownload(url = "https://apps.bridgething.com/r/$v.zip", size = 1, sha256 = SHA),
        permissions = listOf("net.fetch"),
        minLibbridgethingVersion = minLib,
        changelog = null,
    )

private fun app(id: String, name: String, versions: List<CatalogAppVersion>) =
    CatalogApp(id = id, name = name, description = "test", author = "JoeyEamigh", versions = versions)

private fun catalog(apps: List<CatalogApp>) =
    Catalog(
        schema = "catalog.v1",
        updatedAt = "2026-05-31T00:00:00Z",
        repo = CatalogRepo(name = "test", description = "test"),
        apps = apps,
    )

private fun installed(id: String, version: String, source: WebappSource = WebappSource.Installed, role: WebappRole = WebappRole.Standard) =
    WebappInfo(
        id = UUID.fromString(id),
        name = "x",
        source = source,
        role = role,
        version = version,
        iconAvailable = false,
        config = emptyList(),
        permissions = emptyList(),
    )

class SemverCompatTest {
    @Test
    fun `satisfies strips prefix and suffix`() {
        assertTrue(SemverCompat.satisfies("v12.0.1", "12.0.0"))
        assertTrue(SemverCompat.satisfies("12.0.0", "12.0.0"))
        assertFalse(SemverCompat.satisfies("v11.9.9", "12.0.0"))
        assertTrue(SemverCompat.satisfies("v12.1.0-dev", "12.0.0"))
        assertTrue(SemverCompat.satisfies("v2.0.0", "2"))
    }
}

class CatalogDecodeTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun `decodes a catalog`() {
        val raw = """
        {
          "schema": "catalog.v1",
          "updated_at": "2026-05-31T00:00:00Z",
          "repo": { "name": "bridgething apps", "description": "official", "homepage": null, "icon": null },
          "apps": [{
            "id": "$CALENDAR_ID", "name": "Calendar", "description": "Events.", "author": "JoeyEamigh",
            "icon": null, "homepage": null, "source": null,
            "versions": [{
              "version": "0.1.0", "released_at": "2026-05-31T00:00:00Z",
              "download": { "url": "https://apps.bridgething.com/r/x.zip", "size": 10, "sha256": "${"a".repeat(64)}" },
              "permissions": ["net.fetch"], "min_libbridgething_version": "12.0.0", "changelog": "init"
            }]
          }],
          "recommended_sources": [{ "name": "R", "url": "https://r.example.com/catalog.json", "description": null, "attested": true }]
        }
        """.trimIndent()
        val decoded = json.decodeFromString(Catalog.serializer(), raw)
        assertEquals("catalog.v1", decoded.schema)
        assertEquals(1, decoded.apps.size)
        assertEquals(CALENDAR_ID, decoded.apps[0].id)
        assertEquals("12.0.0", decoded.apps[0].versions[0].minLibbridgethingVersion)
        assertEquals(listOf("net.fetch"), decoded.apps[0].versions[0].permissions)
        assertTrue(decoded.recommendedSources[0].attested)
    }
}

class CatalogAggregateTest {
    private fun orderedCatalogs(): List<Pair<String, Catalog>> {
        val a = catalog(listOf(app(CALENDAR_ID, "Calendar", listOf(ver("0.2.0"), ver("0.1.0", released = "2026-04-01T00:00:00Z")))))
        val b = catalog(
            listOf(
                app(CALENDAR_ID, "Calendar", listOf(ver("0.3.0", minLib = "99.0.0"), ver("0.1.5", released = "2026-04-15T00:00:00Z"))),
                app(WEATHER_ID, "Weather", listOf(ver("0.1.0"))),
            )
        )
        return listOf(SOURCE_A to a, SOURCE_B to b)
    }

    @Test
    fun `pinned source is primary and compat filters`() {
        val listings = CatalogService.aggregate(
            orderedCatalogs(),
            listOf(installed(CALENDAR_ID, "0.1.5")),
            mapOf(CALENDAR_ID to SOURCE_B),
            "v12.0.1",
        )
        assertEquals(2, listings.size)
        val cal = listings.first { it.app.id == CALENDAR_ID }
        assertEquals(SOURCE_B, cal.sourceUrl)
        // 0.3.0 needs lib 99.0.0; device is 12.0.1, so the newest compatible is 0.1.5.
        assertEquals("0.1.5", cal.newestCompatible?.version)
        assertEquals("0.1.5", cal.installedVersion)
        assertFalse(cal.updateAvailable)
        assertEquals(listOf(SOURCE_A), cal.alsoAvailableFrom)

        val weather = listings.first { it.app.id == WEATHER_ID }
        assertNull(weather.installedVersion)
        assertEquals("0.1.0", weather.newestCompatible?.version)
        assertTrue(weather.alsoAvailableFrom.isEmpty())
    }

    @Test
    fun `defaults to first source when unpinned`() {
        val listings = CatalogService.aggregate(orderedCatalogs(), emptyList(), emptyMap(), "v12.0.1")
        val cal = listings.first { it.app.id == CALENDAR_ID }
        assertEquals(SOURCE_A, cal.sourceUrl)
        assertEquals("0.2.0", cal.newestCompatible?.version)
        assertEquals(listOf(SOURCE_B), cal.alsoAvailableFrom)
    }

    @Test
    fun `no compatible version for old device`() {
        val a = catalog(listOf(app(CALENDAR_ID, "Calendar", listOf(ver("0.3.0", minLib = "99.0.0")))))
        val listings = CatalogService.aggregate(listOf(SOURCE_A to a), emptyList(), emptyMap(), "v12.0.1")
        assertNull(listings[0].newestCompatible)
    }

    @Test
    fun `null device version lists newest`() {
        val a = catalog(listOf(app(CALENDAR_ID, "Calendar", listOf(ver("0.3.0", minLib = "99.0.0")))))
        val listings = CatalogService.aggregate(listOf(SOURCE_A to a), emptyList(), emptyMap(), null)
        assertEquals("0.3.0", listings[0].newestCompatible?.version)
    }
}

class CatalogUpdatesTest {
    @Test
    fun `offers update only from pinned source`() {
        val a = catalog(listOf(app(CALENDAR_ID, "Calendar", listOf(ver("0.2.0"), ver("0.1.0", released = "2026-04-01T00:00:00Z")))))
        val b = catalog(listOf(app(CALENDAR_ID, "Calendar", listOf(ver("0.3.0"), ver("0.1.0", released = "2026-04-01T00:00:00Z")))))
        val catalogs = mapOf(SOURCE_A to a, SOURCE_B to b)

        // pinned to A: target is A's newest (0.2.0), not B's 0.3.0.
        val updates = CatalogService.updates(
            catalogs, mapOf(CALENDAR_ID to SOURCE_A),
            listOf(installed(CALENDAR_ID, "0.1.0")), "v12.0.1",
        )
        assertEquals(1, updates.size)
        assertEquals("0.2.0", updates[0].target.version)
        assertEquals(SOURCE_A, updates[0].sourceUrl)
        assertEquals("0.1.0", updates[0].installedVersion)
    }

    @Test
    fun `skips unpinned builtin and up to date`() {
        val a = catalog(listOf(app(CALENDAR_ID, "Calendar", listOf(ver("0.2.0")))))
        val catalogs = mapOf(SOURCE_A to a)

        // unpinned installed app: no update offered (provenance unknown).
        assertTrue(CatalogService.updates(catalogs, emptyMap(), listOf(installed(CALENDAR_ID, "0.1.0")), "v12.0.1").isEmpty())
        // builtin: never a catalog app.
        assertTrue(
            CatalogService.updates(
                catalogs, mapOf(CALENDAR_ID to SOURCE_A),
                listOf(installed(CALENDAR_ID, "0.1.0", source = WebappSource.Builtin)), "v12.0.1",
            ).isEmpty()
        )
        // already newest: no update.
        assertTrue(
            CatalogService.updates(
                catalogs, mapOf(CALENDAR_ID to SOURCE_A),
                listOf(installed(CALENDAR_ID, "0.2.0")), "v12.0.1",
            ).isEmpty()
        )
    }
}

class CatalogStoreTest {
    @Test
    fun `in memory round trips`() = runBlocking {
        val store = InMemoryCatalogStore()
        store.saveSources(listOf(SOURCE_A, SOURCE_B))
        store.savePins(mapOf(CALENDAR_ID to SOURCE_B))
        assertEquals(listOf(SOURCE_A, SOURCE_B), store.loadSources())
        assertEquals(mapOf(CALENDAR_ID to SOURCE_B), store.loadPins())
    }

    @Test
    fun `file store round trips`() = runBlocking {
        val dir = File(System.getProperty("java.io.tmpdir"), "btcat-${UUID.randomUUID()}")
        val store = FileCatalogStore(dir)
        store.saveSources(listOf(SOURCE_A))
        store.savePins(mapOf(WEATHER_ID to SOURCE_A))
        val reopened = FileCatalogStore(dir)
        assertEquals(listOf(SOURCE_A), reopened.loadSources())
        assertEquals(mapOf(WEATHER_ID to SOURCE_A), reopened.loadPins())
        dir.deleteRecursively()
    }
}

private object UnusedInstaller : WebappInstaller {
    override suspend fun installWebapp(gateway: BridgethingGateway, deviceId: String, bundlePath: File) =
        WebappInstallResult.Failed("unused")
}

private object UnusedFetcher : CatalogFetcher {
    override suspend fun fetchCatalog(url: String): Catalog = throw IOException("unused")
    override suspend fun download(url: String, destination: File) {}
}

class CatalogSourceManagementTest {
    @Test
    fun `seeds official then add remove`() = runBlocking {
        val svc = CatalogService(
            installer = UnusedInstaller,
            store = InMemoryCatalogStore(),
            fetcher = UnusedFetcher,
            officialCatalogUrl = SOURCE_A,
        )
        assertEquals(listOf(SOURCE_A), svc.sources())
        svc.addSource(SOURCE_B)
        svc.addSource(SOURCE_B) // idempotent
        assertEquals(listOf(SOURCE_A, SOURCE_B), svc.sources())
        svc.removeSource(SOURCE_A)
        assertEquals(listOf(SOURCE_B), svc.sources())
    }
}
