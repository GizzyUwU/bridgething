import BridgethingGateway
import BridgethingSchema
import Foundation

/// Abstraction over the device location source (default: `CoreLocationProvider`).
@MainActor
public protocol GeoLocationProviding: AnyObject {
    var onPosition: ((Position) -> Void)? { get set }
    var onError: ((GeoError) -> Void)? { get set }

    func configure(accuracy: GeoAccuracy)
    func requestAuthorization()
    func startUpdating()
    func stopUpdating()
    func requestOnce()
}

/// @MainActor: CoreLocation delegate callbacks fire on the main thread.
@MainActor
public final class GeoController {
    private var provider: (any GeoLocationProviding)?
    // Set once in init, read only on the main actor.
    private nonisolated(unsafe) let injectedProvider: (any GeoLocationProviding)?

    private var watchTask: Task<Void, Never>?
    private var unwatchTask: Task<Void, Never>?
    private var getOnceTask: Task<Void, Never>?

    private var gatewayRef: BridgethingGateway?
    private var watching: Bool = false
    private var oneShotConts: [CheckedContinuation<Position, Error>] = []

    // nil -> CoreLocation default when available.
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
        for cont in oneShotConts {
            cont.resume(throwing: GeoControllerError.cancelled)
        }
        oneShotConts.removeAll()
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
            let position = try await withCheckedThrowingContinuation { (cont: CheckedContinuation<Position, Error>) in
                oneShotConts.append(cont)
                provider.requestOnce()
            }
            try? await handle.respond(GeoGetOnceReply(position: position))
        } catch {
            let geoErr: GeoError = (error as? GeoFailure)?.error ?? .unavailable
            try? await handle.respondErr(GeoErrorReply(error: geoErr))
        }
    }

    // MARK: - provider callbacks

    private func didUpdate(_ position: Position) {
        if !oneShotConts.isEmpty {
            oneShotConts.removeFirst().resume(returning: position)
        }
        if watching, let gw = gatewayRef {
            Task { try? await gw.geo.position(position) }
        }
    }

    private func didFail(_ error: GeoError) {
        for cont in oneShotConts {
            cont.resume(throwing: GeoFailure(error: error))
        }
        oneShotConts.removeAll()
    }
}

private enum GeoControllerError: Error {
    case cancelled
}

// Wraps a wire `GeoError`  so it can be thrown.
private struct GeoFailure: Error {
    let error: GeoError
}

#if canImport(CoreLocation)
    import CoreLocation

    /// Default `GeoLocationProviding` backed by `CLLocationManager`.
    @MainActor
    public final class CoreLocationProvider: NSObject, GeoLocationProviding, CLLocationManagerDelegate {
        public var onPosition: ((Position) -> Void)?
        public var onError: ((GeoError) -> Void)?

        private lazy var manager: CLLocationManager = {
            let m = CLLocationManager()
            m.delegate = self
            return m
        }()

        override public init() { super.init() }

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

        public func startUpdating() { manager.startUpdatingLocation() }
        public func stopUpdating() { manager.stopUpdatingLocation() }
        public func requestOnce() { manager.requestLocation() }

        public nonisolated func locationManager(_: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
            guard let last = locations.last else { return }
            MainActor.assumeIsolated { self.onPosition?(Self.makePosition(from: last)) }
        }

        public nonisolated func locationManager(_: CLLocationManager, didFailWithError _: Error) {
            MainActor.assumeIsolated { self.onError?(.unavailable) }
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
