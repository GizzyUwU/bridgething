#if canImport(CoreLocation)

    import BridgethingCompanionCore
    import CoreLocation
    import Foundation
    import os

    private let geoLog = Logger(subsystem: "com.bridgething.companion", category: "geo")

    public final class CoreLocationGeoProvider: GeoProvider, @unchecked Sendable {
        private let lock = NSLock()
        private var inbox: GeoInbox?
        private var engine: LocationEngine?

        public init() {}

        public func canProvideLocation() -> Bool {
            onMainSync { self.resolvedEngine().canProvideLocation }
        }

        public func start(inbox: GeoInbox) {
            lock.lock()
            self.inbox = inbox
            lock.unlock()
        }

        public func stop() {
            lock.lock()
            inbox = nil
            lock.unlock()
            DispatchQueue.main.async { [weak self] in self?.engine?.reset() }
        }

        public func configure(accuracy: GeoAccuracy) {
            onMainAsync { $0.configure(accuracy: accuracy) }
        }

        public func requestAuthorization() {
            onMainAsync { $0.requestAuthorization() }
        }

        public func startUpdating() {
            onMainAsync { $0.startUpdating() }
        }

        public func stopUpdating() {
            onMainAsync { $0.stopUpdating() }
        }

        public func requestOnce() {
            onMainAsync { $0.requestOnce() }
        }

        public func cancelOnce() {
            onMainAsync { $0.cancelOnce() }
        }

        // MARK: - engine plumbing

        fileprivate func report(_ deliver: (GeoInbox) -> Void) {
            lock.lock()
            let held = inbox
            lock.unlock()
            if let held { deliver(held) }
        }

        private func resolvedEngine() -> LocationEngine {
            if let engine { return engine }
            let built = LocationEngine(owner: self)
            engine = built
            return built
        }

        private func onMainAsync(_ block: @escaping @Sendable (LocationEngine) -> Void) {
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                block(self.resolvedEngine())
            }
        }

        private func onMainSync<T: Sendable>(_ block: @escaping @Sendable () -> T) -> T {
            if Thread.isMainThread {
                return block()
            }
            return DispatchQueue.main.sync { block() }
        }
    }

    private final class LocationEngine: NSObject, CLLocationManagerDelegate {
        private weak var owner: CoreLocationGeoProvider?

        private lazy var manager: CLLocationManager = {
            let m = CLLocationManager()
            m.delegate = self
            #if os(iOS)
                if Self.hasLocationBackgroundMode {
                    m.allowsBackgroundLocationUpdates = true
                } else {
                    geoLog.error("UIBackgroundModes lacks `location`; background fixes will stop when the app backgrounds")
                }
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

        static let parkedResumeMeters: CLLocationDistance = 200

        private var watchActive = false
        private var oneShotActive = false
        private var parked = false
        private var parkedAnchor: CLLocation?

        init(owner: CoreLocationGeoProvider) {
            self.owner = owner
            super.init()
        }

        var canProvideLocation: Bool {
            Self.canProvide(manager.authorizationStatus)
        }

        func reset() {
            if watchActive { stopUpdating() }
            if oneShotActive { cancelOnce() }
        }

        func configure(accuracy: GeoAccuracy) {
            manager.desiredAccuracy = switch accuracy {
            case .coarse: kCLLocationAccuracyHundredMeters
            case .fine: kCLLocationAccuracyBest
            }
        }

        func requestAuthorization() {
            #if os(iOS)
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

        func startUpdating() {
            watchActive = true
            unpark(restartWatch: false)
            syncBackgroundSession()
            manager.startUpdatingLocation()
        }

        func stopUpdating() {
            watchActive = false
            unpark(restartWatch: false)
            manager.stopUpdatingLocation()
            syncBackgroundSession()
        }

        func requestOnce() {
            oneShotActive = true
            syncBackgroundSession()
            manager.requestLocation()
        }

        func cancelOnce() {
            guard oneShotActive else { return }
            oneShotActive = false
            if !watchActive {
                manager.stopUpdatingLocation()
            }
            syncBackgroundSession()
        }

        private func syncBackgroundSession() {
            #if os(iOS)
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

        static func travelledFarEnough(
            from anchor: CLLocation?,
            to location: CLLocation,
            thresholdMeters: CLLocationDistance = LocationEngine.parkedResumeMeters
        ) -> Bool {
            guard let anchor else { return true }
            return location.distance(from: anchor) > thresholdMeters
        }

        // MARK: - CLLocationManagerDelegate

        func locationManager(_: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
            guard let last = locations.last else { return }
            let servedAOneShot = oneShotActive
            oneShotActive = false
            if parked, !servedAOneShot, Self.travelledFarEnough(from: parkedAnchor, to: last) {
                unpark(restartWatch: true)
            }
            syncBackgroundSession()
            owner?.report { $0.onPosition(position: Self.makePosition(from: last)) }
        }

        func locationManagerDidPauseLocationUpdates(_ manager: CLLocationManager) {
            #if os(iOS)
                guard watchActive, !parked else { return }
                parked = true
                parkedAnchor = manager.location
                self.manager.startMonitoringSignificantLocationChanges()
                syncBackgroundSession()
                geoLog.info("location updates paused while stationary; significant-change monitor armed")
            #endif
        }

        func locationManagerDidResumeLocationUpdates(_: CLLocationManager) {
            unpark(restartWatch: false)
        }

        func locationManager(_: CLLocationManager, didFailWithError error: Error) {
            geoLog.warning("core location failed: \(error.localizedDescription, privacy: .public)")
            oneShotActive = false
            syncBackgroundSession()
            let mapped = mapError(error)
            owner?.report { $0.onError(error: mapped) }
        }

        func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
            let usable = Self.canProvide(manager.authorizationStatus)
            geoLog.info("location authorization changed; usable=\(usable)")
            owner?.report { $0.onAuthorizationChange(granted: usable) }
        }

        private func mapError(_ error: Error) -> GeoError {
            guard let clError = error as? CLError, clError.code == .denied else { return .unavailable }
            return Self.canProvide(manager.authorizationStatus) ? .unavailable : .permissionDenied
        }

        private static func canProvide(_ status: CLAuthorizationStatus) -> Bool {
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
