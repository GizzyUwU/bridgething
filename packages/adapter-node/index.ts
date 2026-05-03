import { type Adapter, type AdapterEvent, type AdapterListener, type Device } from '@bridgething/gateway';

import {
  type AdapterMode,
  type AdapterOptions,
  type AdapterEvent as NativeAdapterEvent,
  NodeAdapter as NativeNodeAdapter,
} from './native/index.js';

export { adapterVersion } from './native/index.js';
export type { AdapterMode, AdapterOptions };

/**
 * Node.js byte-level transport adapter for bridgething. Wraps the napi-rs
 * `NativeNodeAdapter` from `./native/` and exposes the byte-level `Adapter`
 * contract that `BridgethingGateway` consumes - translating between the
 * napi-emitted PascalCase event variants (`Connected/Disconnected/Bytes`)
 * and the gateway's lowercased shape, and coercing the napi `Array<number>`
 * byte payload into a `Uint8Array`.
 *
 * The Rust side handles BR/EDR + RFCOMM discovery, pairing prompts, and the
 * raw read/write loop. The codec (16-byte header, msgpack-named, optional
 * gzip) lives one layer up in `@bridgething/gateway`.
 */
export class NodeAdapter implements Adapter {
  private readonly native: NativeNodeAdapter;
  private readonly listeners: Set<AdapterListener> = new Set();
  private wired = false;

  constructor(options?: AdapterOptions) {
    this.native = new NativeNodeAdapter(options);
  }

  on(listener: AdapterListener): void {
    if (!this.wired) {
      this.native.on(event => this.dispatch(event));
      this.wired = true;
    }
    this.listeners.add(listener);
  }

  off(listener: AdapterListener): void {
    this.listeners.delete(listener);
  }

  start(): Promise<void> {
    return this.native.start();
  }

  stop(): Promise<void> {
    return this.native.stop();
  }

  disconnect(deviceId: string): Promise<void> {
    return this.native.disconnect(deviceId);
  }

  async send(deviceId: string, frame: Uint8Array): Promise<void> {
    // napi-rs Buffer wraps a Uint8Array's underlying memory; copy the slice
    // so we hand the native side a stable, byte-aligned buffer regardless of
    // any byteOffset / shared backing storage on the input.
    const buffer = Buffer.from(frame.buffer, frame.byteOffset, frame.byteLength);
    this.native.send(deviceId, buffer);
  }

  private dispatch(event: NativeAdapterEvent): void {
    const translated = translate(event);
    for (const listener of this.listeners) {
      try {
        listener(translated);
      } catch (err) {
        // eslint-disable-next-line no-console
        console.error('[bridgething] adapter listener threw', err);
      }
    }
  }
}

function translate(event: NativeAdapterEvent): AdapterEvent {
  switch (event.type) {
    case 'Connected': {
      const device: Device = { id: event.device.id, name: event.device.name };
      return { type: 'connected', device };
    }
    case 'Disconnected':
      return { type: 'disconnected', deviceId: event.deviceId };
    case 'Bytes':
      return {
        type: 'bytes',
        deviceId: event.deviceId,
        // napi-rs marshals `Vec<u8>` as `Array<number>`; allocate a typed
        // view once at the boundary so downstream code never sees raw arrays.
        data: Uint8Array.from(event.data),
      };
  }
}
