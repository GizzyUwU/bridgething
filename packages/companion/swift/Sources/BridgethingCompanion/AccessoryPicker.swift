import Foundation

/// Result of the general-purpose `BridgethingCompanion.presentPairPicker()`
/// flow. `nil` means the user cancelled the picker.
public struct AccessoryPickResult: Sendable, Equatable {
    /// Stable identifier for the chosen accessory. On iOS this is the
    /// AccessorySetupKit `bluetoothIdentifier` (CoreBluetooth peripheral
    /// UUID); callers echo it back to the wire protocol.
    public let id: String
    /// Friendly name the picker surfaced. May be empty when the
    /// accessory only advertised an address.
    public let name: String

    public init(id: String, name: String) {
        self.id = id
        self.name = name
    }
}

#if os(iOS)
    import AccessorySetupKit

    /// AccessorySetupKit coordinator for the general "pick a Car Thing"
    /// flow. Surfaces every in-range bridgething accessory in the system
    /// picker and returns the chosen one. No CoreBluetooth follow-up;
    /// the picker tap is the whole flow.
    @available(iOS 18.0, *)
    @MainActor
    final class AccessoryPickerCoordinator: NSObject {
        private let session = ASAccessorySession()
        private var sessionActivated = false
        private var pendingPick: PendingPick?

        struct PendingPick {
            let continuation: CheckedContinuation<AccessoryPickResult?, Never>
        }

        override init() {
            super.init()
        }

        func pick() async -> AccessoryPickResult? {
            await activateIfNeeded()
            return await withCheckedContinuation { (continuation: CheckedContinuation<AccessoryPickResult?, Never>) in
                pendingPick = PendingPick(continuation: continuation)
                showPicker()
            }
        }

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
                guard let accessory = event.accessory else { return }
                completePending(result(from: accessory))
            case .pickerDidDismiss:
                completePendingIfNeeded(nil)
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
                guard let self, error != nil else { return }
                MainActor.assumeIsolated { self.completePendingIfNeeded(nil) }
            }
        }

        private func result(from accessory: ASAccessory) -> AccessoryPickResult {
            let id = accessory.bluetoothIdentifier?.uuidString
                ?? accessory.displayName
            return AccessoryPickResult(id: id, name: accessory.displayName)
        }

        private func completePending(_ value: AccessoryPickResult?) {
            guard let pending = pendingPick else { return }
            pendingPick = nil
            pending.continuation.resume(returning: value)
        }

        private func completePendingIfNeeded(_ value: AccessoryPickResult?) {
            guard pendingPick != nil else { return }
            completePending(value)
        }
    }
#endif
