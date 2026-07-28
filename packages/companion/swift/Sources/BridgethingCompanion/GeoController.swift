import BridgethingGateway
import BridgethingSchema
import Foundation
import Logging

private let geoLog = Logger(label: "com.bridgething.companion.geo")

@MainActor
public protocol GeoLocationProviding: AnyObject {
    var onPosition: ((Position) -> Void)? { get set }
    var onError: ((GeoError) -> Void)? { get set }

    func configure(accuracy: GeoAccuracy)
    func requestAuthorization()
    func startUpdating()
    func stopUpdating()
    func requestOnce()
    func cancelOnce()
}

@MainActor
public final class GeoController {
    private var provider: (any GeoLocationProviding)?
    private nonisolated(unsafe) let injectedProvider: (any GeoLocationProviding)?

    private var watchTask: Task<Void, Never>?
    private var unwatchTask: Task<Void, Never>?
    private var getOnceTask: Task<Void, Never>?

    private var gatewayRef: BridgethingGateway?
    private var watching: Bool = false
    private var oneShots: [OneShot] = []

    static var oneShotTimeout: Duration = .seconds(30)

    private struct OneShot {
        let id: UUID
        let cont: CheckedContinuation<Position, Error>
    }

    public nonisolated init(provider: (any GeoLocationProviding)? = nil) {
        injectedProvider = provider
    }

    private func ensureProvider() -> (any GeoLocationProviding)? {
        if let provider { return provider }
        guard let resolved = injectedProvider ?? Self.makeDefaultProvider() else { return nil }
        resolved.onPosition = { [weak self] position in self?.didUpdate(position) }
        resolved.onError = { [weak self] error in self?.didFail(error) }
        provider = resolved
        return resolved
    }

    private static func makeDefaultProvider() -> (any GeoLocationProviding)? {
        #if canImport(CoreLocation)
            return CoreLocationProvider()
        #else
            return nil
        #endif
    }

    public func start(gateway: BridgethingGateway) async {
        gatewayRef = gateway
        _ = ensureProvider()

        watchTask = Task { [weak self] in
            for await (_, msg) in gateway.geo.watch {
                await self?.handleWatch(msg)
            }
        }
        unwatchTask = Task { [weak self] in
            for await _ in gateway.geo.unwatch {
                await self?.handleUnwatch()
            }
        }
        getOnceTask = Task { [weak self] in
            for await (handle, req) in gateway.geo.getOnceRequests {
                Task { [weak self] in await self?.handleGetOnce(handle: handle, req: req) }
            }
        }
    }

    public func stop() async {
        watchTask?.cancel(); watchTask = nil
        unwatchTask?.cancel(); unwatchTask = nil
        getOnceTask?.cancel(); getOnceTask = nil

        if watching {
            provider?.stopUpdating()
            watching = false
        }
        if !oneShots.isEmpty {
            provider?.cancelOnce()
        }
        for shot in oneShots {
            shot.cont.resume(throwing: GeoControllerError.cancelled)
        }
        oneShots.removeAll()
        gatewayRef = nil
    }

    // MARK: - watch / unwatch

    private func handleWatch(_ watch: GeoWatch) async {
        guard let provider = ensureProvider() else { return }
        provider.requestAuthorization()
        provider.configure(accuracy: watch.accuracy)
        if !watching {
            watching = true
            provider.startUpdating()
        }
    }

    private func handleUnwatch() async {
        if watching {
            provider?.stopUpdating()
            watching = false
        }
    }

    // MARK: - get-once

    private func handleGetOnce(handle: GeoGetOnceHandle, req: GeoGetOnce) async {
        guard let provider = ensureProvider() else {
            try? await handle.respondErr(GeoErrorReply(error: .unavailable))
            return
        }
        provider.requestAuthorization()
        provider.configure(accuracy: req.accuracy)
        do {
            let position = try await awaitOneShot(provider: provider)
            try? await handle.respond(GeoGetOnceReply(position: position))
        } catch {
            let geoErr: GeoError = (error as? GeoFailure)?.error ?? .unavailable
            try? await handle.respondErr(GeoErrorReply(error: geoErr))
        }
    }

    private func awaitOneShot(provider: any GeoLocationProviding) async throws -> Position {
        let id = UUID()
        let deadline = Task { [weak self] in
            try? await Task.sleep(for: Self.oneShotTimeout)
            guard !Task.isCancelled else { return }
            self?.expireOneShot(id: id)
        }
        defer { deadline.cancel() }

        return try await withCheckedThrowingContinuation { (cont: CheckedContinuation<Position, Error>) in
            oneShots.append(OneShot(id: id, cont: cont))
            provider.requestOnce()
        }
    }

    private func expireOneShot(id: UUID) {
        guard let idx = oneShots.firstIndex(where: { $0.id == id }) else { return }
        let shot = oneShots.remove(at: idx)
        geoLog.warning("geo.getOnce timed out with no fix and no error")
        if oneShots.isEmpty {
            provider?.cancelOnce()
        }
        shot.cont.resume(throwing: GeoFailure(error: .unavailable))
    }

    // MARK: - provider callbacks

    private func didUpdate(_ position: Position) {
        if !oneShots.isEmpty {
            oneShots.removeFirst().cont.resume(returning: position)
        }
        if watching, let gw = gatewayRef {
            Task { try? await gw.geo.position(position) }
        }
    }

    private func didFail(_ error: GeoError) {
        if watching, let gw = gatewayRef {
            geoLog.warning("geo watch failed while subscribed: \(String(describing: error))")
            Task { try? await gw.geo.errorEvent(GeoErrorReply(error: error)) }
        }
        for shot in oneShots {
            shot.cont.resume(throwing: GeoFailure(error: error))
        }
        oneShots.removeAll()
    }
}

private enum GeoControllerError: Error {
    case cancelled
}

private struct GeoFailure: Error {
    let error: GeoError
}

#if canImport(CoreLocation)
    import CoreLocation

    @MainActor
    public final class CoreLocationProvider: NSObject, GeoLocationProviding, CLLocationManagerDelegate {
        public var onPosition: ((Position) -> Void)?
        public var onError: ((GeoError) -> Void)?

        private lazy var manager: CLLocationManager = {
            let m = CLLocationManager()
            m.delegate = self
            #if os(iOS)
                if Self.hasLocationBackgroundMode {
                    m.allowsBackgroundLocationUpdates = true
                } else {
                    geoLog.error("UIBackgroundModes lacks `location`; background fixes will stop when the app backgrounds")
                }
                m.pausesLocationUpdatesAutomatically = false
            #endif
            return m
        }()

        #if os(iOS)
            private static let hasLocationBackgroundMode: Bool = {
                let modes = Bundle.main.object(forInfoDictionaryKey: "UIBackgroundModes") as? [String]
                return modes?.contains("location") ?? false
            }()

            private var session: CLBackgroundActivitySession?
        #endif

        private var watchActive = false
        private var oneShotActive = false

        override public init() { super.init() }

        private func syncBackgroundSession() {
            #if os(iOS)
                let needed = watchActive || oneShotActive
                if needed, session == nil {
                    session = CLBackgroundActivitySession()
                    geoLog.info("background activity session started")
                } else if !needed, session != nil {
                    session?.invalidate()
                    session = nil
                    geoLog.info("background activity session ended")
                }
            #endif
        }

        public func configure(accuracy: GeoAccuracy) {
            manager.desiredAccuracy = switch accuracy {
            case .coarse: kCLLocationAccuracyHundredMeters
            case .fine: kCLLocationAccuracyBest
            }
        }

        public func requestAuthorization() {
            if manager.authorizationStatus == .notDetermined {
                manager.requestWhenInUseAuthorization()
            }
        }

        public func startUpdating() {
            watchActive = true
            syncBackgroundSession()
            manager.startUpdatingLocation()
        }

        public func stopUpdating() {
            watchActive = false
            manager.stopUpdatingLocation()
            syncBackgroundSession()
        }

        public func requestOnce() {
            oneShotActive = true
            syncBackgroundSession()
            manager.requestLocation()
        }

        public func cancelOnce() {
            guard oneShotActive else { return }
            oneShotActive = false
            if !watchActive {
                manager.stopUpdatingLocation()
            }
            syncBackgroundSession()
        }

        public nonisolated func locationManager(_: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
            guard let last = locations.last else { return }
            MainActor.assumeIsolated {
                self.oneShotActive = false
                self.syncBackgroundSession()
                self.onPosition?(Self.makePosition(from: last))
            }
        }

        public nonisolated func locationManager(_: CLLocationManager, didFailWithError error: Error) {
            MainActor.assumeIsolated {
                geoLog.warning("core location failed: \(error.localizedDescription)")
                self.oneShotActive = false
                self.syncBackgroundSession()
                self.onError?(Self.mapError(error))
            }
        }

        private static func mapError(_ error: Error) -> GeoError {
            guard let clError = error as? CLError else { return .unavailable }
            return clError.code == .denied ? .permissionDenied : .unavailable
        }

        private static func makePosition(from location: CLLocation) -> Position {
            Position(
                lat: location.coordinate.latitude,
                lon: location.coordinate.longitude,
                altM: location.verticalAccuracy >= 0 ? Float(location.altitude) : nil,
                accuracyM: Float(location.horizontalAccuracy.isFinite ? max(location.horizontalAccuracy, 0) : 0),
                speedMps: location.speed >= 0 ? Float(location.speed) : nil,
                headingDeg: location.course >= 0 ? Float(location.course) : nil,
                tsUnixS: UInt32(max(location.timestamp.timeIntervalSince1970, 0))
            )
        }
    }
#endif
