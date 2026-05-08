import BridgethingSchema
import Foundation

/// One-shot outcome of `BridgethingCompanion.enableAncsNotifications()`.
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

    /// Service UUIDs the daemon advertises and hosts for the ANCS LE-bond
    /// bring-up. These match `bridgething/crates/core/src/bluetooth/ancs/pair_trigger.rs`
    /// byte-for-byte; do not change one without the other.
    public enum AncsBluetooth {
        nonisolated(unsafe) public static let pairTriggerService =
            CBUUID(string: "B12BE732-C1D0-4001-8001-BB1D6E7A1C01")
        nonisolated(unsafe) public static let pairTriggerChar =
            CBUUID(string: "B12BE732-C1D0-4001-8001-BB1D6E7A1C02")
        public static let advertisedName = "Bridgething"
    }

    /// AccessorySetupKit + CoreBluetooth coordinator for the ANCS LE-pair
    /// flow. iAP2 already paired BR/EDR; iOS won't initiate
    /// SMP-over-BR/EDR cross-transport key derivation against an existing
    /// SC bond, so the second LE pair has to be driven from the iPhone
    /// side. ASK does it cleanly on iOS 18+: the picker tap performs
    /// SMP, after which the app's CBCentralManager retrieves the
    /// peripheral by `bluetoothIdentifier` and connects with
    /// `CBConnectPeripheralOptionRequiresANCS = true` so iOS surfaces
    /// the ANCS authorization prompt while the app is foreground.
    ///
    /// iOS 13–17 path is intentionally not implemented; the host app
    /// targets iOS 18.
    @available(iOS 18.0, *)
    @MainActor
    final class AncsPairCoordinator: NSObject {
        private let session = ASAccessorySession()
        private var central: CBCentralManager?
        private var centralDelegate: CentralDelegate?
        private var sessionActivated = false
        private var pendingPair: PendingPair?
        private var lastAuthState: AncsAuthState = .unknown

        struct PendingPair {
            let continuation: CheckedContinuation<AncsSetupResult, Never>
        }

        override init() {
            super.init()
        }

        func setLastAuthState(_ state: AncsAuthState) {
            lastAuthState = state
        }

        /// Run the ASK pair flow. Returns once the picker-side outcome is
        /// known (paired / alreadyPaired / cancelled / failed). The
        /// daemon-observed ANCS authorization state may transition
        /// asynchronously after — the caller subscribes to the wire
        /// stream for the final word.
        func pair() async -> AncsSetupResult {
            await activateIfNeeded()

            if hasMatchingExistingAccessory() {
                let accessory = currentAccessory()
                if let accessory {
                    triggerAncsConnect(for: accessory)
                }
                return AncsSetupResult(kind: .alreadyPaired, authState: lastAuthState)
            }

            return await withCheckedContinuation { (continuation: CheckedContinuation<AncsSetupResult, Never>) in
                pendingPair = PendingPair(continuation: continuation)
                showPicker()
            }
        }

        // MARK: - ASK plumbing

        private func activateIfNeeded() async {
            if sessionActivated { return }
            sessionActivated = true
            session.activate(on: .main) { [weak self] event in
                guard let self else { return }
                MainActor.assumeIsolated { self.handleSessionEvent(event) }
            }
        }

        private func handleSessionEvent(_ event: ASAccessoryEvent) {
            switch event.eventType {
            case .accessoryAdded:
                if let accessory = event.accessory {
                    triggerAncsConnect(for: accessory)
                    completePending(.paired)
                }
            case .accessoryRemoved:
                // User revoked from Settings or via removeAccessory.
                lastAuthState = .unknown
            case .pickerDidDismiss:
                // Resolves cancellations. If `accessoryAdded` already
                // fired and resolved the continuation, this is a no-op.
                completePendingIfNeeded(.cancelled)
            default:
                break
            }
        }

        private func showPicker() {
            let descriptor = ASDiscoveryDescriptor()
            descriptor.bluetoothServiceUUID = AncsBluetooth.pairTriggerService
            descriptor.bluetoothNameSubstring = AncsBluetooth.advertisedName

            let item = ASPickerDisplayItem(
                name: AncsBluetooth.advertisedName,
                productImage: UIImage(),
                descriptor: descriptor
            )
            session.showPicker(for: [item]) { [weak self] error in
                guard let self else { return }
                if let error {
                    MainActor.assumeIsolated {
                        self.completePending(.failed(String(describing: error)))
                    }
                }
            }
        }

        private func hasMatchingExistingAccessory() -> Bool {
            currentAccessory() != nil
        }

        private func currentAccessory() -> ASAccessory? {
            session.accessories.first {
                $0.descriptor.bluetoothServiceUUID == AncsBluetooth.pairTriggerService
            }
        }

        private func completePending(_ kind: AncsSetupKind) {
            guard let pending = pendingPair else { return }
            pendingPair = nil
            pending.continuation.resume(returning: AncsSetupResult(kind: kind, authState: lastAuthState))
        }

        private func completePendingIfNeeded(_ kind: AncsSetupKind) {
            guard pendingPair != nil else { return }
            completePending(kind)
        }

        // MARK: - CoreBluetooth: connect with RequiresANCS to fire the auth prompt

        private func triggerAncsConnect(for accessory: ASAccessory) {
            guard let identifier = accessory.bluetoothIdentifier else { return }
            ensureCentralManager()
            guard let central, let delegate = centralDelegate else { return }
            delegate.targetIdentifier = identifier
            delegate.attemptConnect()
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

        nonisolated func centralManagerDidUpdateState(_ central: CBCentralManager) {
            MainActor.assumeIsolated { self.attemptConnect() }
        }

        func attemptConnect() {
            guard let central, central.state == .poweredOn,
                  let id = targetIdentifier,
                  let peripheral = central.retrievePeripherals(withIdentifiers: [id]).first
            else { return }
            connecting = peripheral
            peripheral.delegate = self
            // `CBConnectPeripheralOptionRequiresANCS = true` tells iOS to
            // gate the connection on ANCS being exposed. After SMP (which
            // ASK already drove on the picker tap) iOS pops the
            // notification-sharing prompt while the app is foreground.
            // String key — no Swift constant for it.
            let options: [String: Any] = ["kCBConnectOptionRequiresANCS": NSNumber(value: true)]
            central.connect(peripheral, options: options)
        }

        // CB delivers all delegate callbacks on the main queue we configured,
        // so `MainActor.assumeIsolated` below is a thread-checked no-op. The
        // peripheral itself is held in `connecting` and re-looked-up on the
        // MainActor side rather than passed across the isolation boundary
        // (CBPeripheral is not Sendable under Swift 6 strict concurrency).
        nonisolated func centralManager(_: CBCentralManager, didConnect _: CBPeripheral) {
            MainActor.assumeIsolated { self.handleDidConnect() }
        }

        @MainActor
        private func handleDidConnect() {
            connecting?.discoverServices([AncsBluetooth.pairTriggerService])
        }

        nonisolated func centralManager(
            _: CBCentralManager,
            didFailToConnect _: CBPeripheral,
            error _: Error?
        ) {
            // Connection failures here are non-fatal — the daemon will
            // eventually observe ANCS authorization (or not) and emit
            // the wire event.
        }

        nonisolated func centralManager(_: CBCentralManager, didDisconnectPeripheral _: CBPeripheral, error _: Error?) {}

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
            // Idempotent encrypt-read. ASK already drove SMP, so this
            // succeeds; on the rare path where SMP didn't complete it
            // forces it. Empty value either way.
            p.readValue(for: ch)
        }

        nonisolated func peripheral(
            _: CBPeripheral,
            didUpdateValueFor _: CBCharacteristic,
            error _: Error?
        ) {}
    }
#endif
