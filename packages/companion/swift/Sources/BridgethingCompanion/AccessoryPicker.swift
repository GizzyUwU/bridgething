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
