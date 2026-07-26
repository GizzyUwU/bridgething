import BridgethingSchema
import Foundation

public enum AncsSetupKind: Sendable, Equatable {
    case paired
    case alreadyPaired
    case cancelled
    case unsupported
    case failed(String)
}

public struct AncsSetupResult: Sendable {
    public let kind: AncsSetupKind
    public let authState: AncsAuthState

    public init(kind: AncsSetupKind, authState: AncsAuthState) {
        self.kind = kind
        self.authState = authState
    }
}

#if os(iOS)
    import AccessorySetupKit
    import CoreBluetooth
    import Logging
    import UIKit

    private let askLog = Logger(label: "com.bridgething.companion.ancs")

    public enum AncsBluetooth {
        nonisolated(unsafe) public static let pairTriggerService =
            CBUUID(string: "B12BE732-C1D0-4001-8001-BB1D6E7A1C01")
        nonisolated(unsafe) public static let pairTriggerChar =
            CBUUID(string: "B12BE732-C1D0-4001-8001-BB1D6E7A1C02")

        public static let productLabel = "Car Thing"

        public static func advertisedName(serial: String) -> String {
            "\(productLabel) (SN: \(serial.suffix(4)))"
        }
    }

    @available(iOS 18.0, *)
    @MainActor
    final class AncsPairCoordinator: NSObject {
        private let session = ASAccessorySession()
        private var central: CBCentralManager?
        private var centralDelegate: CentralDelegate?
        private var sessionActivated = false
        private var activationContinuation: CheckedContinuation<Void, Never>?
        private var pendingPairs: [String: [CheckedContinuation<AncsSetupResult, Never>]] = [:]
        private var authStates: [String: AncsAuthState] = [:]

        override init() {
            super.init()
        }

        func setAuthState(serial: String, _ state: AncsAuthState) {
            authStates[AncsBluetooth.advertisedName(serial: serial)] = state
        }

        func pair(serial: String) async -> AncsSetupResult {
            let name = AncsBluetooth.advertisedName(serial: serial)
            askLog.info("pair() begin for \(name)")
            await activateIfNeeded()
            guard sessionActivated else {
                askLog.error("pair() aborting: session never activated")
                return AncsSetupResult(kind: .failed("accessory session failed to activate"), authState: authState(name))
            }

            if let accessory = currentAccessory(name: name) {
                askLog.info("pair() already holds \(name)")
                triggerConnect(for: accessory, requiresAncs: true)
                return AncsSetupResult(kind: .alreadyPaired, authState: authState(name))
            }

            let alreadyShowing = pendingPairs[name] != nil
            return await withCheckedContinuation { (continuation: CheckedContinuation<AncsSetupResult, Never>) in
                pendingPairs[name, default: []].append(continuation)
                if !alreadyShowing { showPicker(name: name) }
            }
        }

        func reconnectIfPaired(serial: String) async {
            await activateIfNeeded()
            let name = AncsBluetooth.advertisedName(serial: serial)
            guard sessionActivated, let accessory = currentAccessory(name: name) else { return }
            triggerConnect(for: accessory, requiresAncs: false)
        }

        func hasPairedAccessory(serial: String) async -> Bool {
            await activateIfNeeded()
            let name = AncsBluetooth.advertisedName(serial: serial)
            return sessionActivated && currentAccessory(name: name) != nil
        }

        // MARK: - ASK plumbing

        private func activateIfNeeded() async {
            if sessionActivated { return }
            askLog.info("activating ASAccessorySession")
            await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
                activationContinuation = continuation
                session.activate(on: .main) { [weak self] event in
                    guard let self else { return }
                    MainActor.assumeIsolated { self.handleSessionEvent(event) }
                }
                Task { @MainActor [weak self] in
                    try? await Task.sleep(nanoseconds: 8_000_000_000)
                    guard let self, self.activationContinuation != nil else { return }
                    askLog.error("activation timed out: no .activated event in 8s")
                    self.finishActivation()
                }
            }
        }

        private func finishActivation() {
            guard let continuation = activationContinuation else { return }
            activationContinuation = nil
            continuation.resume()
        }

        private func handleSessionEvent(_ event: ASAccessoryEvent) {
            askLog.info("session event: \(String(describing: event.eventType))")
            switch event.eventType {
            case .activated:
                sessionActivated = true
                finishActivation()
            case .invalidated:
                finishActivation()
            case .accessoryAdded:
                if let accessory = event.accessory {
                    triggerConnect(for: accessory, requiresAncs: true)
                    completePending(pendingKey(for: accessory) ?? accessory.displayName, .paired)
                }
            case .accessoryRemoved:
                if let accessory = event.accessory {
                    authStates[accessory.displayName] = nil
                    if let key = accessory.descriptor.bluetoothNameSubstring { authStates[key] = nil }
                }
            case .pickerDidDismiss:
                completeAllPending(.cancelled)
            default:
                break
            }
        }

        private func showPicker(name: String) {
            let descriptor = ASDiscoveryDescriptor()
            descriptor.bluetoothServiceUUID = AncsBluetooth.pairTriggerService
            descriptor.bluetoothNameSubstring = name

            let item = ASPickerDisplayItem(
                name: name,
                productImage: Self.pickerImage(),
                descriptor: descriptor
            )
            askLog.info("showPicker presenting for \(name)")
            session.showPicker(for: [item]) { [weak self] error in
                guard let self else { return }
                if let error {
                    askLog.error("showPicker error: \(String(describing: error))")
                    MainActor.assumeIsolated {
                        self.completePending(name, .failed(String(describing: error)))
                    }
                } else {
                    askLog.info("showPicker completion: no error")
                }
            }
        }

        private static func pickerImage() -> UIImage {
            if let icon = UIImage(named: "AncsPickerIcon", in: .module, compatibleWith: nil) {
                return icon
            }
            let size = CGSize(width: 60, height: 60)
            return UIGraphicsImageRenderer(size: size).image { _ in
                UIColor(red: 0.0, green: 0.6, blue: 0.86, alpha: 1.0).setFill()
                UIBezierPath(roundedRect: CGRect(origin: .zero, size: size), cornerRadius: 13).fill()
            }
        }

        private func currentAccessory(name: String) -> ASAccessory? {
            session.accessories.first {
                guard $0.descriptor.bluetoothServiceUUID == AncsBluetooth.pairTriggerService else { return false }
                return $0.descriptor.bluetoothNameSubstring == name || $0.displayName == name
            }
        }

        private func pendingKey(for accessory: ASAccessory) -> String? {
            pendingPairs.keys.first {
                accessory.descriptor.bluetoothNameSubstring == $0 || accessory.displayName == $0
            }
        }

        private func authState(_ name: String) -> AncsAuthState {
            authStates[name] ?? .unknown
        }

        private func completePending(_ name: String, _ kind: AncsSetupKind) {
            guard let pending = pendingPairs.removeValue(forKey: name) else { return }
            let result = AncsSetupResult(kind: kind, authState: authState(name))
            for continuation in pending {
                continuation.resume(returning: result)
            }
        }

        private func completeAllPending(_ kind: AncsSetupKind) {
            let pending = pendingPairs
            pendingPairs = [:]
            for (name, continuations) in pending {
                let result = AncsSetupResult(kind: kind, authState: authState(name))
                for continuation in continuations { continuation.resume(returning: result) }
            }
        }

        // MARK: - CoreBluetooth: bring the LE link up; RequiresANCS only for the explicit pair flow

        private func triggerConnect(for accessory: ASAccessory, requiresAncs: Bool) {
            guard let identifier = accessory.bluetoothIdentifier else { return }
            ensureCentralManager()
            guard let delegate = centralDelegate else { return }
            delegate.targetIdentifier = identifier
            delegate.connect(requiringAncs: requiresAncs)
        }

        private func ensureCentralManager() {
            if central != nil { return }
            let delegate = CentralDelegate()
            centralDelegate = delegate
            central = CBCentralManager(delegate: delegate, queue: .main)
            delegate.central = central
        }
    }

    @available(iOS 18.0, *)
    @MainActor
    private final class CentralDelegate: NSObject, CBCentralManagerDelegate, CBPeripheralDelegate {
        weak var central: CBCentralManager?
        var targetIdentifier: UUID?
        private var connecting: CBPeripheral?
        private var requiresAncs = false
        private var ancsFallbackTask: Task<Void, Never>?
        private var scanFallbackTask: Task<Void, Never>?
        private var retryTask: Task<Void, Never>?

        private static let ancsGateDeadline: UInt64 = 10_000_000_000
        private static let scanDeadline: UInt64 = 8_000_000_000

        func connect(requiringAncs: Bool) {
            if requiringAncs { requiresAncs = true }
            attemptConnect()
        }

        nonisolated func centralManagerDidUpdateState(_ central: CBCentralManager) {
            MainActor.assumeIsolated { self.attemptConnect() }
        }

        func attemptConnect() {
            guard let central, central.state == .poweredOn, targetIdentifier != nil else { return }
            if let p = connecting {
                if p.state == .connected { return }
                if p.state == .connecting, requiresAncs { return }
                if p.state == .connecting {
                    askLog.warning("re-driving over a wedged LE connect; cancelling stale attempt before rescanning")
                }
                cancelInFlightConnect()
            }
            askLog.info("scanning for pair-trigger service to LE-connect")
            central.scanForPeripherals(withServices: [AncsBluetooth.pairTriggerService], options: nil)
            armScanFallback()
        }

        private func cancelInFlightConnect() {
            ancsFallbackTask?.cancel()
            ancsFallbackTask = nil
            retryTask?.cancel()
            retryTask = nil
            if let peripheral = connecting {
                central?.cancelPeripheralConnection(peripheral)
            }
            connecting = nil
        }

        nonisolated func centralManager(
            _: CBCentralManager,
            didDiscover peripheral: CBPeripheral,
            advertisementData _: [String: Any],
            rssi _: NSNumber
        ) {
            let discovered = peripheral.identifier
            MainActor.assumeIsolated { self.handleDidDiscover(discovered) }
        }

        @MainActor
        private func handleDidDiscover(_ identifier: UUID) {
            guard identifier == targetIdentifier else { return }
            guard let central, let peripheral = central.retrievePeripherals(withIdentifiers: [identifier]).first
            else { return }
            askLog.info("discovered target over LE; connecting (requiresAncs=\(self.requiresAncs))")
            central.stopScan()
            scanFallbackTask?.cancel()
            scanFallbackTask = nil
            issueConnect(to: peripheral)
        }

        private func armScanFallback() {
            scanFallbackTask?.cancel()
            scanFallbackTask = Task { @MainActor [weak self] in
                try? await Task.sleep(nanoseconds: Self.scanDeadline)
                guard let self, !Task.isCancelled else { return }
                self.central?.stopScan()
                guard let central = self.central,
                      let id = self.targetIdentifier,
                      let peripheral = central.retrievePeripherals(withIdentifiers: [id]).first
                else { return }
                askLog.warning("LE discovery silent; falling back to retrieve-based connect")
                self.issueConnect(to: peripheral)
            }
        }

        private func issueConnect(to peripheral: CBPeripheral) {
            guard let central else { return }
            connecting = peripheral
            peripheral.delegate = self
            if requiresAncs {
                let options: [String: Any] = ["kCBConnectOptionRequiresANCS": NSNumber(value: true)]
                central.connect(peripheral, options: options)
                armAncsGateFallback()
            } else {
                central.connect(peripheral, options: nil)
            }
        }

        private func armAncsGateFallback() {
            ancsFallbackTask?.cancel()
            ancsFallbackTask = Task { @MainActor [weak self] in
                try? await Task.sleep(nanoseconds: Self.ancsGateDeadline)
                guard let self, !Task.isCancelled else { return }
                guard let peripheral = self.connecting, peripheral.state != .connected else { return }
                askLog.warning("RequiresANCS connect stuck in \(peripheral.state.rawValue); falling back to plain connect")
                self.requiresAncs = false
                self.central?.cancelPeripheralConnection(peripheral)
                self.issueConnect(to: peripheral)
            }
        }

        nonisolated func centralManager(_: CBCentralManager, didConnect _: CBPeripheral) {
            MainActor.assumeIsolated { self.handleDidConnect() }
        }

        @MainActor
        private func handleDidConnect() {
            askLog.info("LE link connected (requiresAncs=\(self.requiresAncs))")
            ancsFallbackTask?.cancel()
            ancsFallbackTask = nil
            scanFallbackTask?.cancel()
            scanFallbackTask = nil
            central?.stopScan()
            requiresAncs = false
            connecting?.discoverServices([AncsBluetooth.pairTriggerService])
        }

        nonisolated func centralManager(
            _: CBCentralManager,
            didFailToConnect _: CBPeripheral,
            error: Error?
        ) {
            MainActor.assumeIsolated { self.handleDidFail(error: error) }
        }

        @MainActor
        private func handleDidFail(error: Error?) {
            askLog.warning("LE connect failed (\(String(describing: error))); retrying plain")
            requiresAncs = false
            scheduleRetry()
        }

        nonisolated func centralManager(_: CBCentralManager, didDisconnectPeripheral _: CBPeripheral, error: Error?) {
            MainActor.assumeIsolated { self.handleDidDisconnect(error: error) }
        }

        @MainActor
        private func handleDidDisconnect(error: Error?) {
            askLog.warning("LE link dropped (\(String(describing: error))); reconnecting")
            requiresAncs = false
            attemptConnect()
        }

        private func scheduleRetry() {
            retryTask?.cancel()
            retryTask = Task { @MainActor [weak self] in
                try? await Task.sleep(nanoseconds: 2_000_000_000)
                guard let self, !Task.isCancelled else { return }
                self.attemptConnect()
            }
        }

        nonisolated func peripheral(_: CBPeripheral, didDiscoverServices _: Error?) {
            MainActor.assumeIsolated { self.handleDidDiscoverServices() }
        }

        @MainActor
        private func handleDidDiscoverServices() {
            guard let p = connecting,
                  let svc = p.services?.first(where: { $0.uuid == AncsBluetooth.pairTriggerService })
            else { return }
            p.discoverCharacteristics([AncsBluetooth.pairTriggerChar], for: svc)
        }

        nonisolated func peripheral(
            _: CBPeripheral,
            didDiscoverCharacteristicsFor _: CBService,
            error _: Error?
        ) {
            MainActor.assumeIsolated { self.handleDidDiscoverCharacteristics() }
        }

        @MainActor
        private func handleDidDiscoverCharacteristics() {
            guard let p = connecting,
                  let svc = p.services?.first(where: { $0.uuid == AncsBluetooth.pairTriggerService }),
                  let ch = svc.characteristics?.first(where: { $0.uuid == AncsBluetooth.pairTriggerChar })
            else { return }
            p.readValue(for: ch)
        }

        nonisolated func peripheral(
            _: CBPeripheral,
            didUpdateValueFor _: CBCharacteristic,
            error _: Error?
        ) {}
    }
#endif
