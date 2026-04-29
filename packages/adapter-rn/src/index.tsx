import { type Adapter, type AdapterEvent, type AdapterListener } from '@bridgething/gateway';

import type { BridgethingTransport, BridgethingTransportDevice } from './specs/BridgethingTransport.nitro';

export class ReactNativeAdapterError extends Error {
  constructor(
    message: string,
    public readonly kind: 'not-started' | 'unknown-device' | 'send-failed' | 'transport',
  ) {
    super(message);
    this.name = 'ReactNativeAdapterError';
  }
}

export type ReactNativeAdapterOptions = {
  /**
   * Override the underlying Nitro HybridObject. Tests use a fake; production
   * leaves this unset and lets the wrapper resolve the autolinked native
   * implementation by name.
   */
  transport?: BridgethingTransport;
};

/**
 * Byte-level RN adapter built on top of the Nitro HybridObject. Implements
 * the `Adapter` contract that `BridgethingGateway` expects: bytes in, bytes
 * out, plus connect / disconnect / connected events.
 *
 * On iOS the underlying transport observes `EAAccessoryDidConnect` and
 * auto-opens sessions for the bridgething protocol string - `connect()` is
 * a manual fallback for accessories that were already attached when the
 * adapter started. On Android, `connect(deviceId)` opens the RFCOMM channel
 * to a bonded `BluetoothDevice`.
 */
export class ReactNativeAdapter implements Adapter {
  private readonly transport: BridgethingTransport;
  private readonly listeners: Set<AdapterListener> = new Set();
  private started = false;

  constructor(options: ReactNativeAdapterOptions = {}) {
    this.transport = options.transport ?? createNativeTransport();
    this.wireListeners();
  }

  on(listener: AdapterListener): void {
    this.listeners.add(listener);
  }

  off(listener: AdapterListener): void {
    this.listeners.delete(listener);
  }

  async start(): Promise<void> {
    if (this.started) return;
    this.started = true;
    await this.transport.start();
  }

  async stop(): Promise<void> {
    if (!this.started) return;
    this.started = false;
    await this.transport.stop();
  }

  async disconnect(deviceId: string): Promise<void> {
    await this.transport.disconnect(deviceId);
  }

  async send(deviceId: string, frame: Uint8Array): Promise<void> {
    if (!this.started) {
      throw new ReactNativeAdapterError('adapter not started', 'not-started');
    }
    try {
      await this.transport.send(deviceId, toArrayBuffer(frame));
    } catch (err) {
      throw new ReactNativeAdapterError(errorMessage(err), 'send-failed');
    }
  }

  /**
   * Manually open a session against a known peer.
   *
   * iOS: useful when the accessory was already attached before `start()` -
   * `EAAccessoryDidConnect` doesn't fire retroactively. Returns immediately
   * if iOS already has a session for `deviceId`.
   *
   * Android: required for every bonded `BluetoothDevice` - RFCOMM doesn't
   * auto-open. The device must already be paired via system Settings.
   */
  async connect(deviceId: string): Promise<BridgethingTransportDevice> {
    if (!this.started) {
      throw new ReactNativeAdapterError('adapter not started', 'not-started');
    }
    return this.transport.connect(deviceId);
  }

  /** Snapshot of currently-connectable peers known to the OS. */
  getKnownDevices(): BridgethingTransportDevice[] {
    return this.transport.getKnownDevices();
  }

  private wireListeners(): void {
    this.transport.setOnConnected(device => {
      this.dispatch({ type: 'connected', device: { id: device.id, name: device.name } });
    });
    this.transport.setOnDisconnected(deviceId => {
      this.dispatch({ type: 'disconnected', deviceId });
    });
    this.transport.setOnBytes((deviceId, frame) => {
      this.dispatch({ type: 'bytes', deviceId, data: new Uint8Array(frame) });
    });
    this.transport.setOnError((deviceId, description) => {
      // Errors from the native transport bubble up to consumers via the
      // gateway's existing decode-error path is for *codec* failures;
      // transport-level errors aren't part of the Adapter event surface, so
      // surface them through console for now and rely on the disconnect
      // event that follows.
      console.warn(`[bridgething] transport error on ${deviceId}: ${description}`);
    });
  }

  private dispatch(event: AdapterEvent): void {
    for (const listener of this.listeners) {
      try {
        listener(event);
      } catch (err) {
        console.error('[bridgething] adapter listener threw', err);
      }
    }
  }
}

function createNativeTransport(): BridgethingTransport {
  // Lazy-require so test environments that pass `options.transport` never
  // pull in the RN runtime (Bun can't parse react-native's Flow source).
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const { NitroModules } = require('react-native-nitro-modules') as typeof import('react-native-nitro-modules');
  return NitroModules.createHybridObject<BridgethingTransport>('BridgethingTransport');
}

function toArrayBuffer(bytes: Uint8Array): ArrayBuffer {
  if (bytes.byteOffset === 0 && bytes.byteLength === bytes.buffer.byteLength) {
    return bytes.buffer as ArrayBuffer;
  }
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  return copy.buffer;
}

function errorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  return String(err);
}

export type { BridgethingTransport, BridgethingTransportDevice } from './specs/BridgethingTransport.nitro';
