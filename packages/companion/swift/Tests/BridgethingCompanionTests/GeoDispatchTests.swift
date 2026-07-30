import BridgethingGateway
import BridgethingSchema
import BridgethingTestKit
import CoreLocation
import XCTest

@testable import BridgethingCompanion

@MainActor
final class GeoDispatchTests: XCTestCase {
    final class FakeGeoProvider: GeoLocationProviding {
        var onPosition: ((Position) -> Void)?
        var onError: ((GeoError) -> Void)?
        var onAuthorizationChange: ((Bool) -> Void)?

        let canned: Position
        var nextError: GeoError?
        var swallowOnce = false
        var cancelOnceCalls = 0
        var failOnStart: GeoError?
        var canProvideLocation = true

        init(canned: Position) { self.canned = canned }

        /// Drive the authorization callback the way CoreLocation's delegate would.
        func setCanProvideLocation(_ usable: Bool) {
            canProvideLocation = usable
            onAuthorizationChange?(usable)
        }

        func configure(accuracy _: GeoAccuracy) {}
        func requestAuthorization() {}

        func startUpdating() {
            if let err = failOnStart { onError?(err); return }
            onPosition?(canned)
        }
        func stopUpdating() {}
        func requestOnce() {
            if swallowOnce { return }
            if let err = nextError { nextError = nil; onError?(err); return }
            onPosition?(canned)
        }

        func cancelOnce() { cancelOnceCalls += 1 }
    }

    private func boot(provider: FakeGeoProvider) async throws -> (BridgethingCompanion, WireDriver) {
        let adapter = InMemoryAdapter()
        let companion = BridgethingCompanion(
            adapter: adapter,
            lyricsResolver: FakeLyricsResolver(),
            host: HostInfo(appName: "geo-test", appVersion: "0.0.1", osName: "macOS"),
            geoProvider: provider
        )
        try await companion.start()
        let driver = WireDriver(adapter: adapter)
        await driver.start()
        driver.connect()
        return (companion, driver)
    }

    private static let fix = Position(
        lat: 40.7128, lon: -74.0060, altM: nil, accuracyM: 8, speedMps: nil, headingDeg: nil, tsUnixS: 1_700_000_000
    )

    func testGetOnceRoutesToProvider() async throws {
        let (companion, driver) = try await boot(provider: FakeGeoProvider(canned: Self.fix))
        let resp = try await driver.request(.geo(.getOnce(GeoGetOnce(accuracy: .fine))), timeout: .seconds(3))
        guard case let .geo(.getOnceReply(reply)) = resp.data else {
            return XCTFail("expected getOnceReply, got \(resp.data)")
        }
        XCTAssertEqual(reply.position.lat, Self.fix.lat, accuracy: 0.0001)
        XCTAssertEqual(reply.position.lon, Self.fix.lon, accuracy: 0.0001)
        await companion.stop()
    }

    func testGetOnceProviderErrorMapsToErrorReply() async throws {
        let provider = FakeGeoProvider(canned: Self.fix)
        provider.nextError = .permissionDenied
        let (companion, driver) = try await boot(provider: provider)
        let resp = try await driver.request(.geo(.getOnce(GeoGetOnce(accuracy: .fine))), timeout: .seconds(3))
        guard case let .geo(.errorReply(reply)) = resp.data else {
            return XCTFail("expected geo errorReply, got \(resp.data)")
        }
        XCTAssertEqual(reply.error, .permissionDenied)
        await companion.stop()
    }

    func testGetOnceTimesOutWhenProviderGoesSilent() async throws {
        let provider = FakeGeoProvider(canned: Self.fix)
        provider.swallowOnce = true
        let previous = GeoController.oneShotTimeout
        GeoController.oneShotTimeout = .milliseconds(150)
        defer { GeoController.oneShotTimeout = previous }

        let (companion, driver) = try await boot(provider: provider)
        let resp = try await driver.request(.geo(.getOnce(GeoGetOnce(accuracy: .fine))), timeout: .seconds(3))
        guard case let .geo(.errorReply(reply)) = resp.data else {
            return XCTFail("expected geo errorReply, got \(resp.data)")
        }
        XCTAssertEqual(reply.error, .unavailable)
        XCTAssertEqual(provider.cancelOnceCalls, 1)
        await companion.stop()
    }

    func testWatchBroadcastsPosition() async throws {
        let (companion, driver) = try await boot(provider: FakeGeoProvider(canned: Self.fix))
        try await driver.send(.geo(.watch(GeoWatch(accuracy: .fine, minIntervalMs: 0))))
        let frame = try await driver.waitOutbound { msg in
            if case .geo(.position) = msg.data { return true }
            return false
        }
        guard case let .geo(.position(position)) = frame.data else {
            return XCTFail("expected a geo position broadcast, got \(frame.data)")
        }
        XCTAssertEqual(position.lat, Self.fix.lat, accuracy: 0.0001)
        await companion.stop()
    }

    func testLosingAuthorizationReannouncesGeoAsUnavailable() async throws {
        let provider = FakeGeoProvider(canned: Self.fix)
        let (companion, driver) = try await boot(provider: provider)

        let first = try await driver.waitOutbound { msg in
            if case .capabilities(.announce) = msg.data { return true }
            return false
        }
        guard case let .capabilities(.announce(caps)) = first.data else {
            return XCTFail("expected a capabilities announce, got \(first.data)")
        }
        XCTAssertTrue(caps.available.geo, "geo should be advertised while location is usable")

        provider.setCanProvideLocation(false)

        let next = try await driver.waitOutbound { msg in
            if case let .capabilities(.announce(caps)) = msg.data { return !caps.available.geo }
            return false
        }
        guard case let .capabilities(.announce(revoked)) = next.data else {
            return XCTFail("expected a re-announce, got \(next.data)")
        }
        XCTAssertFalse(revoked.available.geo, "a revoked grant must stop advertising geo")
        await companion.stop()
    }

    // MARK: - parked-watch resume predicate

    func testTheCachedFixReplayedWhenArmingTheMonitorDoesNotCountAsMovement() {
        let parkedAt = CLLocation(latitude: 52.5, longitude: 6.1)
        // core location replays the last known fix the instant significant-change monitoring is armed;
        // treating that as movement would restart the watch and loop straight back into a pause
        XCTAssertFalse(CoreLocationProvider.travelledFarEnough(from: parkedAt, to: parkedAt))
    }

    func testGpsJitterAtAStandstillDoesNotCountAsMovement() {
        let parkedAt = CLLocation(latitude: 52.5, longitude: 6.1)
        let jittered = CLLocation(latitude: 52.500_3, longitude: 6.100_2) // ~35m
        XCTAssertFalse(CoreLocationProvider.travelledFarEnough(from: parkedAt, to: jittered))
    }

    func testDrivingOffCountsAsMovement() {
        let parkedAt = CLLocation(latitude: 52.5, longitude: 6.1)
        let movedOn = CLLocation(latitude: 52.508, longitude: 6.1) // ~890m, past the monitor's ~500m
        XCTAssertTrue(CoreLocationProvider.travelledFarEnough(from: parkedAt, to: movedOn))
    }

    func testWithoutAnAnchorAnyFixHasToCountAsMovement() {
        let somewhere = CLLocation(latitude: 52.5, longitude: 6.1)
        XCTAssertTrue(CoreLocationProvider.travelledFarEnough(from: nil, to: somewhere))
    }

    func testWatchFailureEmitsErrorEvent() async throws {
        let provider = FakeGeoProvider(canned: Self.fix)
        provider.failOnStart = .permissionDenied
        let (companion, driver) = try await boot(provider: provider)
        try await driver.send(.geo(.watch(GeoWatch(accuracy: .fine, minIntervalMs: 0))))
        let frame = try await driver.waitOutbound { msg in
            if case .geo(.errorEvent) = msg.data { return true }
            return false
        }
        guard case let .geo(.errorEvent(reply)) = frame.data else {
            return XCTFail("expected a geo error event, got \(frame.data)")
        }
        XCTAssertEqual(reply.error, .permissionDenied)
        await companion.stop()
    }
}
