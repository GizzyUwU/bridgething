import Foundation

/// Identifier and human-readable name for a connected bridgething peer.
///
/// `id` is opaque to the gateway and only meaningful to the underlying adapter
/// (`EAAccessory.serialNumber` on iOS, BluetoothDevice address on Android,
/// peripheral UUID for BLE, etc.). Pass it back to `Adapter.send` /
/// `Adapter.disconnect` to address that specific peer.
public struct Device: Sendable, Equatable, Hashable {
  public let id: String
  public let name: String

  public init(id: String, name: String) {
    self.id = id
    self.name = name
  }
}

/// Raw byte-level events surfaced by an `Adapter` to the gateway.
///
/// The gateway accumulates `bytes` chunks per device into framed payloads;
/// adapters do not need to align chunks to frame boundaries.
public enum AdapterEvent: Sendable {
  case connected(Device)
  case disconnected(deviceId: String)
  case bytes(deviceId: String, Data)
}

/// Errors an adapter can surface to its consumer.
public enum AdapterError: Error, Sendable {
  case notStarted
  case unknownDevice(String)
  case sendFailed(String)
  case transport(String)
}

/// Byte-level transport contract. Implementations plumb a specific Bluetooth
/// stack (EAAccessory on iOS, BluetoothSocket on Android, BLE elsewhere) and
/// emit raw chunks; framing, gzip, and msgpack live one layer up in the
/// gateway, not here.
///
/// Multi-device by design: a single `Adapter` instance can manage several
/// concurrent peers, addressed by the opaque `deviceId` from `Device.id`.
public protocol Adapter: AnyObject, Sendable {
  var events: AsyncStream<AdapterEvent> { get }

  func start() async throws
  func stop() async
  func disconnect(deviceId: String) async throws
  func send(deviceId: String, frame: Data) async throws
}
