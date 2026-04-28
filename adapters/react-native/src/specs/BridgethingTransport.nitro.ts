import type { HybridObject } from 'react-native-nitro-modules';

/**
 * Wire-side identity for a connected bridgething peer. `id` is opaque to the
 * gateway and only meaningful to the underlying transport — `EAAccessory.serialNumber`
 * (or `connectionID` fallback) on iOS, `BluetoothDevice.address` on Android.
 */
export type BridgethingTransportDevice = {
  id: string;
  name: string;
};

/**
 * Transport-only contract between the RN adapter (TS side) and the native
 * EA/RFCOMM session manager. The native side ships raw bytes; framing, gzip,
 * and msgpack live in `@bridgething/lib` + `@bridgething/gateway` on the JS
 * side. Nitro maps this interface to a Swift `protocol` (iOS) and Kotlin
 * `interface` (Android) at codegen time; consumers don't touch either.
 *
 * Single-listener-per-event by design: fan-out to multiple JS subscribers
 * happens in the TS wrapper one layer up.
 */
export interface BridgethingTransport extends HybridObject<{ ios: 'swift'; android: 'kotlin' }> {
  /**
   * Begin observing the underlying transport.
   *
   * iOS: register for `EAAccessoryDidConnect`/`EAAccessoryDidDisconnect`.
   * Android: prepare internal state; explicit `connect(deviceId)` calls open
   * RFCOMM sessions afterwards.
   */
  start(): Promise<void>;

  /** Tear down all sessions and stop observing. */
  stop(): Promise<void>;

  /**
   * iOS: no-op (sessions auto-open when an EAAccessory connects with the
   * matching protocol string). Returns the EAAccessory device once iOS
   * surfaces it.
   *
   * Android: open an RFCOMM channel to the bonded `BluetoothDevice` whose
   * MAC is `deviceId`. The device must already be paired via system settings.
   */
  connect(deviceId: string): Promise<BridgethingTransportDevice>;

  /** Close the session for `deviceId`. */
  disconnect(deviceId: string): Promise<void>;

  /**
   * Write a fully-framed bridgething wire frame to the peer. The buffer is
   * read synchronously (it's non-owning); callers can reuse it after this
   * promise resolves.
   */
  send(deviceId: string, frame: ArrayBuffer): Promise<void>;

  /**
   * Snapshot of currently-connectable peers known to the OS:
   * - iOS: registered EAAccessories matching our protocol string
   * - Android: bonded BluetoothDevices (filtering by service is the consumer's
   *   responsibility — the Bluetooth API exposes UUIDs only after connect on
   *   most stacks)
   */
  getKnownDevices(): BridgethingTransportDevice[];

  setOnConnected(callback: (device: BridgethingTransportDevice) => void): void;
  setOnDisconnected(callback: (deviceId: string) => void): void;
  setOnBytes(callback: (deviceId: string, frame: ArrayBuffer) => void): void;
  setOnError(callback: (deviceId: string, description: string) => void): void;
}
