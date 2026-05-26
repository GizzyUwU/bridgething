import type { HybridObject } from 'react-native-nitro-modules';

/**
 * Wire-side peer identity. `id` is opaque to the gateway: `EAAccessory.serialNumber`
 * (or `connectionID` fallback) on iOS, `BluetoothDevice.address` on Android.
 */
export type BridgethingTransportDevice = {
  id: string;
  name: string;
};

/**
 * Transport contract between the RN adapter (TS) and the native EA/RFCOMM session manager.
 * Native ships raw bytes; framing and codec live in `@bridgething/lib` + `@bridgething/gateway`.
 * Nitro maps this to Swift/Kotlin at codegen time.
 *
 * Single-listener-per-event; fan-out happens in the TS wrapper.
 */
export interface BridgethingTransport extends HybridObject<{ ios: 'swift'; android: 'kotlin' }> {
  /**
   * Begin observing the underlying transport. iOS registers for EA accessory events;
   * Android prepares state and waits for explicit `connect(deviceId)` calls.
   */
  start(): Promise<void>;

  /** Tear down all sessions and stop observing. */
  stop(): Promise<void>;

  /**
   * iOS: no-op; sessions auto-open on EAAccessory connect. Returns the device once surfaced.
   * Android: open an RFCOMM channel to the bonded device at `deviceId` (must be system-paired).
   */
  connect(deviceId: string): Promise<BridgethingTransportDevice>;

  /** Close the session for `deviceId`. */
  disconnect(deviceId: string): Promise<void>;

  /** Write a framed wire frame to the peer. The buffer is read synchronously; callers can reuse it after resolve. */
  send(deviceId: string, frame: ArrayBuffer): Promise<void>;

  /**
   * Snapshot of currently-connectable peers known to the OS. iOS returns registered EAAccessories;
   * Android returns bonded devices (service filtering is the caller's responsibility, as UUIDs are
   * only exposed after connect on most stacks).
   *
   * Async because the reads are platform-isolated (main-thread on iOS, binder bridge on Android).
   */
  getKnownDevices(): Promise<BridgethingTransportDevice[]>;

  setOnConnected(callback: (device: BridgethingTransportDevice) => void): void;
  setOnDisconnected(callback: (deviceId: string) => void): void;
  setOnBytes(callback: (deviceId: string, frame: ArrayBuffer) => void): void;
  setOnError(callback: (deviceId: string, description: string) => void): void;
}
