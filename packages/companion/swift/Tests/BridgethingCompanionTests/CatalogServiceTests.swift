import BridgethingGateway
import BridgethingSchema
import Foundation
import XCTest

@testable import BridgethingCompanion

private let CALENDAR_ID = "019e6701-13f8-71b5-ba04-85d326630e98"
private let WEATHER_ID = "019e6701-13f8-71b5-ba04-81f347137de2"
private let SOURCE_A = URL(string: "https://apps.bridgething.com/catalog.json")!
private let SOURCE_B = URL(string: "https://repo.example.com/catalog.json")!

private func download(_ url: String, _ size: Int = 1, _ sha: String = String(repeating: "0", count: 64)) -> CatalogDownload {
    CatalogDownload(url: url, size: size, sha256: sha)
}

private func ver(_ v: String, minLib: String = "12.0.0", released: String = "2026-05-31T00:00:00Z") -> CatalogAppVersion {
    CatalogAppVersion(
        version: v,
        releasedAt: released,
        download: download("https://apps.bridgething.com/r/\(v).zip"),
        permissions: ["net.fetch"],
        minLibbridgethingVersion: minLib,
        changelog: nil
    )
}

private func makeApp(_ id: String, _ name: String, _ versions: [CatalogAppVersion]) -> CatalogApp {
    CatalogApp(id: id, name: name, description: "test", author: "JoeyEamigh", icon: nil, homepage: nil, source: nil, versions: versions)
}

private func makeCatalog(_ apps: [CatalogApp]) -> Catalog {
    Catalog(schema: "catalog.v1", updatedAt: "2026-05-31T00:00:00Z",
            repo: CatalogRepo(name: "test", description: "test", homepage: nil, icon: nil),
            apps: apps, recommendedSources: [])
}

private func installed(_ id: String, _ ver: String, source: WebappSource = .installed, role: WebappRole = .standard) -> WebappInfo {
    WebappInfo(id: UUID(uuidString: id)!, name: "x", source: source, role: role, version: ver,
               description: nil, iconAvailable: false, iconMime: nil, config: [], permissions: [], voiceGrammar: nil)
}

final class SemverCompatTests: XCTestCase {
    func testSatisfiesStripsPrefixAndSuffix() {
        XCTAssertTrue(SemverCompat.satisfies(deviceVersion: "v12.0.1", minimum: "12.0.0"))
        XCTAssertTrue(SemverCompat.satisfies(deviceVersion: "12.0.0", minimum: "12.0.0"))
        XCTAssertFalse(SemverCompat.satisfies(deviceVersion: "v11.9.9", minimum: "12.0.0"))
        XCTAssertTrue(SemverCompat.satisfies(deviceVersion: "v12.1.0-dev", minimum: "12.0.0"))
        XCTAssertTrue(SemverCompat.satisfies(deviceVersion: "v2.0.0", minimum: "2"))
    }
}

final class CatalogDecodeTests: XCTestCase {
    func testDecodesACatalog() throws {
        let json = """
        {
          "schema": "catalog.v1",
          "updated_at": "2026-05-31T00:00:00Z",
          "repo": { "name": "bridgething apps", "description": "official", "homepage": null, "icon": null },
          "apps": [{
            "id": "\(CALENDAR_ID)", "name": "Calendar", "description": "Events.", "author": "JoeyEamigh",
            "icon": null, "homepage": null, "source": null,
            "versions": [{
              "version": "0.1.0", "released_at": "2026-05-31T00:00:00Z",
              "download": { "url": "https://apps.bridgething.com/r/x.zip", "size": 10, "sha256": "\(String(repeating: "a", count: 64))" },
              "permissions": ["net.fetch"], "min_libbridgething_version": "12.0.0", "changelog": "init"
            }]
          }],
          "recommended_sources": [{ "name": "R", "url": "https://r.example.com/catalog.json", "description": null, "attested": true }]
        }
        """
        let decoded = try JSONDecoder().decode(Catalog.self, from: Data(json.utf8))
        XCTAssertEqual(decoded.schema, "catalog.v1")
        XCTAssertEqual(decoded.apps.count, 1)
        XCTAssertEqual(decoded.apps[0].id, CALENDAR_ID)
        XCTAssertEqual(decoded.apps[0].versions[0].minLibbridgethingVersion, "12.0.0")
        XCTAssertEqual(decoded.apps[0].versions[0].permissions, ["net.fetch"])
        XCTAssertTrue(decoded.recommendedSources[0].attested)
    }
}

final class CatalogAggregateTests: XCTestCase {
    private func orderedCatalogs() -> [(url: URL, catalog: Catalog)] {
        let a = makeCatalog([makeApp(CALENDAR_ID, "Calendar", [ver("0.2.0"), ver("0.1.0", released: "2026-04-01T00:00:00Z")])])
        let b = makeCatalog([
            makeApp(CALENDAR_ID, "Calendar", [ver("0.3.0", minLib: "99.0.0"), ver("0.1.5", released: "2026-04-15T00:00:00Z")]),
            makeApp(WEATHER_ID, "Weather", [ver("0.1.0")]),
        ])
        return [(SOURCE_A, a), (SOURCE_B, b)]
    }

    func testPinnedSourceIsPrimaryAndCompatFilters() {
        let listings = CatalogService.aggregate(
            orderedCatalogs: orderedCatalogs(),
            installed: [installed(CALENDAR_ID, "0.1.5")],
            pins: [CALENDAR_ID: SOURCE_B],
            deviceLibVersion: "v12.0.1"
        )
        XCTAssertEqual(listings.count, 2)
        let cal = try! XCTUnwrap(listings.first { $0.app.id == CALENDAR_ID })
        XCTAssertEqual(cal.sourceURL, SOURCE_B)
        // 0.3.0 needs lib 99.0.0; device is 12.0.1, so the newest compatible is 0.1.5.
        XCTAssertEqual(cal.newestCompatible?.version, "0.1.5")
        XCTAssertEqual(cal.installedVersion, "0.1.5")
        XCTAssertFalse(cal.updateAvailable)
        XCTAssertEqual(cal.alsoAvailableFrom, [SOURCE_A])

        let weather = try! XCTUnwrap(listings.first { $0.app.id == WEATHER_ID })
        XCTAssertNil(weather.installedVersion)
        XCTAssertEqual(weather.newestCompatible?.version, "0.1.0")
        XCTAssertTrue(weather.alsoAvailableFrom.isEmpty)
    }

    func testDefaultsToFirstSourceWhenUnpinned() {
        let listings = CatalogService.aggregate(
            orderedCatalogs: orderedCatalogs(),
            installed: [],
            pins: [:],
            deviceLibVersion: "v12.0.1"
        )
        let cal = try! XCTUnwrap(listings.first { $0.app.id == CALENDAR_ID })
        XCTAssertEqual(cal.sourceURL, SOURCE_A)
        XCTAssertEqual(cal.newestCompatible?.version, "0.2.0")
        XCTAssertEqual(cal.alsoAvailableFrom, [SOURCE_B])
    }

    func testNoCompatibleVersionForOldDevice() {
        let a = makeCatalog([makeApp(CALENDAR_ID, "Calendar", [ver("0.3.0", minLib: "99.0.0")])])
        let listings = CatalogService.aggregate(
            orderedCatalogs: [(SOURCE_A, a)], installed: [], pins: [:], deviceLibVersion: "v12.0.1"
        )
        XCTAssertNil(listings[0].newestCompatible)
    }

    func testNilDeviceVersionListsNewest() {
        let a = makeCatalog([makeApp(CALENDAR_ID, "Calendar", [ver("0.3.0", minLib: "99.0.0")])])
        let listings = CatalogService.aggregate(
            orderedCatalogs: [(SOURCE_A, a)], installed: [], pins: [:], deviceLibVersion: nil
        )
        XCTAssertEqual(listings[0].newestCompatible?.version, "0.3.0")
    }
}

final class CatalogUpdatesTests: XCTestCase {
    func testOffersUpdateOnlyFromPinnedSource() {
        let a = makeCatalog([makeApp(CALENDAR_ID, "Calendar", [ver("0.2.0"), ver("0.1.0", released: "2026-04-01T00:00:00Z")])])
        let b = makeCatalog([makeApp(CALENDAR_ID, "Calendar", [ver("0.3.0"), ver("0.1.0", released: "2026-04-01T00:00:00Z")])])
        let catalogs: [URL: Catalog] = [SOURCE_A: a, SOURCE_B: b]

        // pinned to A: target is A's newest (0.2.0), not B's 0.3.0.
        let updates = CatalogService.updates(
            catalogs: catalogs, pins: [CALENDAR_ID: SOURCE_A],
            installed: [installed(CALENDAR_ID, "0.1.0")], deviceLibVersion: "v12.0.1"
        )
        XCTAssertEqual(updates.count, 1)
        XCTAssertEqual(updates[0].target.version, "0.2.0")
        XCTAssertEqual(updates[0].sourceURL, SOURCE_A)
        XCTAssertEqual(updates[0].installedVersion, "0.1.0")
    }

    func testSkipsUnpinnedBuiltinAndUpToDate() {
        let a = makeCatalog([makeApp(CALENDAR_ID, "Calendar", [ver("0.2.0")])])
        let catalogs: [URL: Catalog] = [SOURCE_A: a]

        // unpinned installed app: no update offered (provenance unknown).
        XCTAssertTrue(CatalogService.updates(catalogs: catalogs, pins: [:],
            installed: [installed(CALENDAR_ID, "0.1.0")], deviceLibVersion: "v12.0.1").isEmpty)
        // builtin: never a catalog app.
        XCTAssertTrue(CatalogService.updates(catalogs: catalogs, pins: [CALENDAR_ID: SOURCE_A],
            installed: [installed(CALENDAR_ID, "0.1.0", source: .builtin)], deviceLibVersion: "v12.0.1").isEmpty)
        // already newest: no update.
        XCTAssertTrue(CatalogService.updates(catalogs: catalogs, pins: [CALENDAR_ID: SOURCE_A],
            installed: [installed(CALENDAR_ID, "0.2.0")], deviceLibVersion: "v12.0.1").isEmpty)
    }
}

final class CatalogStoreTests: XCTestCase {
    func testInMemoryRoundTrips() async {
        let store = InMemoryCatalogStore()
        await store.saveSources([SOURCE_A, SOURCE_B])
        await store.savePins([CALENDAR_ID: SOURCE_B])
        let s = await store.loadSources()
        let p = await store.loadPins()
        XCTAssertEqual(s, [SOURCE_A, SOURCE_B])
        XCTAssertEqual(p, [CALENDAR_ID: SOURCE_B])
    }

    func testFileStoreRoundTrips() async {
        let dir = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("btcat-\(UUID().uuidString)")
        let store = FileCatalogStore(directory: dir)
        await store.saveSources([SOURCE_A])
        await store.savePins([WEATHER_ID: SOURCE_A])
        let reopened = FileCatalogStore(directory: dir)
        let s = await reopened.loadSources()
        let p = await reopened.loadPins()
        XCTAssertEqual(s, [SOURCE_A])
        XCTAssertEqual(p, [WEATHER_ID: SOURCE_A])
        try? FileManager.default.removeItem(at: dir)
    }
}

private struct UnusedInstaller: WebappInstaller {
    func installWebapp(gateway _: BridgethingGateway, deviceId _: String, bundlePath _: URL) async -> WebappInstallResult {
        .failed(reason: "unused")
    }
}

private struct UnusedFetcher: CatalogFetcher {
    func fetchCatalog(_: URL) async throws -> Catalog { throw CatalogFetchError.httpStatus(0) }
    func download(_: URL, to _: URL) async throws {}
}

final class CatalogSourceManagementTests: XCTestCase {
    func testSeedsOfficialThenAddRemove() async {
        let svc = CatalogService(
            installer: UnusedInstaller(),
            store: InMemoryCatalogStore(),
            fetcher: UnusedFetcher(),
            officialCatalogURL: SOURCE_A
        )
        var sources = await svc.sources()
        XCTAssertEqual(sources, [SOURCE_A])
        await svc.addSource(SOURCE_B)
        await svc.addSource(SOURCE_B) // idempotent
        sources = await svc.sources()
        XCTAssertEqual(sources, [SOURCE_A, SOURCE_B])
        await svc.removeSource(SOURCE_A)
        sources = await svc.sources()
        XCTAssertEqual(sources, [SOURCE_B])
    }
}
