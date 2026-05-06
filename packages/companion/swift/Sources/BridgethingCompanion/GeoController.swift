#if canImport(CoreLocation)
    import BridgethingGateway
    import BridgethingSchema
    import CoreLocation
    import Foundation

    /// Geo surface implementation backed by `CoreLocation`.
    ///
    /// `CLLocationManager` requires main-thread isolation (delegate
    /// callbacks fire there) so the controller is `@MainActor`-isolated.
    /// `BridgethingCompanion` reaches in via `await`.
    @MainActor
    public final class GeoController {
        private let manager: CLLocationManager
        private let delegate: Delegate

        private var watchTask: Task<Void, Never>?
        private var unwatchTask: Task<Void, Never>?
        private var getOnceTask: Task<Void, Never>?

        private var gatewayRef: BridgethingGateway?
        private var watching: Bool = false
        private var oneShotConts: [CheckedContinuation<CLLocation, Error>] = []

        public nonisolated init() {
            manager = CLLocationManager()
            delegate = Delegate()
            manager.delegate = delegate
        }

        public func start(gateway: BridgethingGateway) async {
            delegate.owner = self
            gatewayRef = gateway

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
                    await self?.handleGetOnce(handle: handle, req: req)
                }
            }
        }

        public func stop() async {
            watchTask?.cancel(); watchTask = nil
            unwatchTask?.cancel(); unwatchTask = nil
            getOnceTask?.cancel(); getOnceTask = nil

            if watching {
                manager.stopUpdatingLocation()
                watching = false
            }
            for cont in oneShotConts {
                cont.resume(throwing: GeoControllerError.cancelled)
            }
            oneShotConts.removeAll()
            delegate.owner = nil
            gatewayRef = nil
        }

        // MARK: - watch / unwatch

        private func handleWatch(_ watch: GeoWatch) async {
            ensureAuthorized()
            manager.desiredAccuracy = Self.accuracy(for: watch.accuracy)
            if !watching {
                watching = true
                manager.startUpdatingLocation()
            }
        }

        private func handleUnwatch() async {
            if watching {
                manager.stopUpdatingLocation()
                watching = false
            }
        }

        // MARK: - get-once

        private func handleGetOnce(handle: GeoGetOnceHandle, req: GeoGetOnce) async {
            ensureAuthorized()
            manager.desiredAccuracy = Self.accuracy(for: req.accuracy)
            do {
                let loc = try await withCheckedThrowingContinuation { (cont: CheckedContinuation<CLLocation, Error>) in
                    oneShotConts.append(cont)
                    manager.requestLocation()
                }
                let position = Self.makePosition(from: loc)
                try? await handle.respond(GeoGetOnceReply(position: position))
            } catch {
                let geoErr: GeoError = (error as? GeoControllerError)
                    .map { Self.mapError($0) } ?? .unavailable
                try? await handle.respondErr(GeoErrorReply(error: geoErr))
            }
        }

        // MARK: - delegate routing

        fileprivate func didUpdateLocation(_ location: CLLocation) {
            if !oneShotConts.isEmpty {
                let cont = oneShotConts.removeFirst()
                cont.resume(returning: location)
            }
            if watching, let gw = gatewayRef {
                let position = Self.makePosition(from: location)
                Task {
                    try? await gw.geo.position(position)
                }
            }
        }

        fileprivate func didFail(_ error: Error) {
            for cont in oneShotConts {
                cont.resume(throwing: GeoControllerError.failed(error))
            }
            oneShotConts.removeAll()
        }

        // MARK: - helpers

        private func ensureAuthorized() {
            let status = manager.authorizationStatus
            if status == .notDetermined {
                manager.requestWhenInUseAuthorization()
            }
        }

        private static func accuracy(for accuracy: GeoAccuracy) -> CLLocationAccuracy {
            switch accuracy {
            case .coarse: kCLLocationAccuracyHundredMeters
            case .fine: kCLLocationAccuracyBest
            }
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

        private static func mapError(_ err: GeoControllerError) -> GeoError {
            switch err {
            case .denied: .permissionDenied
            case .unavailable: .unavailable
            case .cancelled, .failed: .unavailable
            }
        }
    }

    private enum GeoControllerError: Error {
        case denied
        case unavailable
        case cancelled
        case failed(Error)
    }

    @MainActor
    private final class Delegate: NSObject, CLLocationManagerDelegate {
        weak var owner: GeoController?

        nonisolated func locationManager(_: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
            guard let last = locations.last else { return }
            MainActor.assumeIsolated {
                self.owner?.didUpdateLocation(last)
            }
        }

        nonisolated func locationManager(_: CLLocationManager, didFailWithError error: Error) {
            MainActor.assumeIsolated {
                self.owner?.didFail(error)
            }
        }
    }
#endif
