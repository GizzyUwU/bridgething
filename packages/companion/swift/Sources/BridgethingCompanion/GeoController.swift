import BridgethingGateway
import BridgethingSchema
import Foundation
import Logging

private let geoLog = Logger(label: "com.bridgething.companion.geo")

@MainActor
public protocol GeoLocationProviding: AnyObject {
    var onPosition: ((Position) -> Void)? { get set }
    var onError: ((GeoError) -> Void)? { get set }
    var onAuthorizationChange: ((Bool) -> Void)? { get set }

    /// False only when location is definitively unusable - the user refused, or policy forbids it.
    /// Not-yet-determined counts as usable, because a request still raises the prompt.
    var canProvideLocation: Bool { get }

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
    private var onAuthorizationChange: ((Bool) -> Void)?

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
        resolved.onAuthorizationChange = { [weak self] usable in self?.onAuthorizationChange?(usable) }
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

    public func start(gateway: BridgethingGateway, onAuthorizationChange: ((Bool) -> Void)? = nil) async {
        gatewayRef = gateway
        self.onAuthorizationChange = onAuthorizationChange
        let provider = ensureProvider()
        // seed the caller before any fix is asked for, so the first capability announce is honest.
        // ios only: reading this builds the real CLLocationManager, and doing that on the macOS test
        // host couples every unrelated suite to that machine's location grant.
        #if os(iOS)
            if let provider { onAuthorizationChange?(provider.canProvideLocation) }
        #endif

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
        onAuthorizationChange = nil
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
        public var onAuthorizationChange: ((Bool) -> Void)?

        private lazy var manager: CLLocationManager = {
            let m = CLLocationManager()
            m.delegate = self
            #if os(iOS)
                if Self.hasLocationBackgroundMode {
                    m.allowsBackgroundLocationUpdates = true
                } else {
                    geoLog.error("UIBackgroundModes lacks `location`; background fixes will stop when the app backgrounds")
                }
                // automotive lets core location power down radios once the car stops, and pausing is
                // only safe because we ask for always-authorization: under when-in-use a pause ends
                // location access until the app is relaunched, whereas always can re-arm from the
                // background via the significant-change monitor
                m.activityType = .automotiveNavigation
                m.pausesLocationUpdatesAutomatically = true
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

        /// Significant-change monitoring only notifies at roughly 500m, and arming it replays a cached
        /// fix from the same spot straight away. Anything past this is real travel rather than that
        /// replay or gps noise.
        nonisolated static let parkedResumeMeters: CLLocationDistance = 200

        /// Whether a fix arriving while parked means the car actually moved. A nil anchor means we could
        /// not record where it stopped, so any fix has to count.
        nonisolated static func travelledFarEnough(
            from anchor: CLLocation?,
            to location: CLLocation,
            thresholdMeters: CLLocationDistance = CoreLocationProvider.parkedResumeMeters
        ) -> Bool {
            guard let anchor else { return true }
            return location.distance(from: anchor) > thresholdMeters
        }

        private var watchActive = false
        private var oneShotActive = false
        // updates are paused because the car stopped; a cheap monitor is armed to catch it moving off
        private var parked = false
        private var parkedAnchor: CLLocation?

        override public init() { super.init() }

        private func syncBackgroundSession() {
            #if os(iOS)
                // the session IS the status-bar location pill, and an always-authorized app reaches the
                // background without one, so only a when-in-use grant pays that cost
                let needed = (watchActive || oneShotActive) && manager.authorizationStatus != .authorizedAlways
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

        public var canProvideLocation: Bool {
            Self.canProvide(manager.authorizationStatus)
        }

        public func requestAuthorization() {
            #if os(iOS)
                // always-authorization is what lets a locked phone answer a cold request. when-in-use
                // only reaches the background by holding the location indicator up for the whole drive,
                // so it would cost battery and a permanent status-bar pill even while nothing wants a fix.
                switch manager.authorizationStatus {
                case .notDetermined, .authorizedWhenInUse: manager.requestAlwaysAuthorization()
                default: break
                }
            #else
                if manager.authorizationStatus == .notDetermined {
                    manager.requestWhenInUseAuthorization()
                }
            #endif
        }

        public func startUpdating() {
            watchActive = true
            unpark(restartWatch: false)
            syncBackgroundSession()
            manager.startUpdatingLocation()
        }

        public func stopUpdating() {
            watchActive = false
            unpark(restartWatch: false)
            manager.stopUpdatingLocation()
            syncBackgroundSession()
        }

        /// Drop the low-power monitor. `restartWatch` brings the real watch back, which is what a
        /// movement notification wants and what an explicit stop does not.
        private func unpark(restartWatch: Bool) {
            #if os(iOS)
                guard parked else { return }
                parked = false
                parkedAnchor = nil
                manager.stopMonitoringSignificantLocationChanges()
                if restartWatch, watchActive {
                    manager.startUpdatingLocation()
                    geoLog.info("movement detected; full location updates restarted")
                }
            #endif
        }

        private func travelledFarEnoughToResume(_ location: CLLocation) -> Bool {
            Self.travelledFarEnough(from: parkedAnchor, to: location)
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
                let servedAOneShot = self.oneShotActive
                self.oneShotActive = false
                // while parked these come from the significant-change monitor; a one-shot's own fix is
                // not evidence the car moved, and neither is the cached fix arming the monitor replays
                if self.parked, !servedAOneShot, self.travelledFarEnoughToResume(last) {
                    self.unpark(restartWatch: true)
                }
                self.syncBackgroundSession()
                self.onPosition?(Self.makePosition(from: last))
            }
        }

        public nonisolated func locationManagerDidPauseLocationUpdates(_ manager: CLLocationManager) {
            let anchor = manager.location
            MainActor.assumeIsolated {
                #if os(iOS)
                    guard self.watchActive, !self.parked else { return }
                    self.parked = true
                    self.parkedAnchor = anchor
                    self.manager.startMonitoringSignificantLocationChanges()
                    // the watch is still live from the daemon's point of view, it just costs nothing now
                    self.syncBackgroundSession()
                    geoLog.info("location updates paused while stationary; significant-change monitor armed")
                #endif
            }
        }

        public nonisolated func locationManagerDidResumeLocationUpdates(_: CLLocationManager) {
            MainActor.assumeIsolated { self.unpark(restartWatch: false) }
        }

        public nonisolated func locationManager(_: CLLocationManager, didFailWithError error: Error) {
            MainActor.assumeIsolated {
                geoLog.warning("core location failed: \(error.localizedDescription)")
                self.oneShotActive = false
                self.syncBackgroundSession()
                self.onError?(self.mapError(error))
            }
        }

        public nonisolated func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
            // read the status here rather than in the hop; CLLocationManager is not Sendable
            let usable = Self.canProvide(manager.authorizationStatus)
            MainActor.assumeIsolated {
                geoLog.info("location authorization changed; usable=\(usable)")
                self.onAuthorizationChange?(usable)
            }
        }

        private func mapError(_ error: Error) -> GeoError {
            guard let clError = error as? CLError, clError.code == .denied else { return .unavailable }
            // core location also reports `denied` for an authorized app that merely is not in use
            // right now (backgrounded, screen locked), which is transient and not a user refusal
            return Self.canProvide(manager.authorizationStatus) ? .unavailable : .permissionDenied
        }

        private nonisolated static func canProvide(_ status: CLAuthorizationStatus) -> Bool {
            switch status {
            case .denied, .restricted: false
            default: true
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
    }
#endif
