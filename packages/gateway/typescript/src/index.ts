import {
  type BridgeToGatewayMsg,
  Codec,
  FrameAccumulator,
  type GatewayToBridgeMsg,
  type GatewayToBridgeMsgData,
  Logger,
  LogLevel,
  newUuidBytes,
  type Priority,
} from '@bridgething/lib';

import { version } from './version';

export type Device = {
  id: string;
  name: string;
};

export type AdapterEvent =
  | { type: 'connected'; device: Device }
  | { type: 'disconnected'; deviceId: string }
  | { type: 'bytes'; deviceId: string; data: Uint8Array };

export type AdapterListener = (event: AdapterEvent) => void;

/**
 * Byte-level transport contract. Implementations plumb a specific Bluetooth
 * stack (EAAccessory on iOS, BluetoothSocket on Android, BLE elsewhere via the
 * Nitro module, plain BLE via ble-plx for legacy callers, etc.) and emit raw
 * chunks; framing, gzip, and msgpack live one layer up in the gateway.
 *
 * Multi-device by design: a single `Adapter` instance can manage several
 * concurrent peers, addressed by the opaque `deviceId` from `Device.id`.
 */
export interface Adapter {
  on(listener: AdapterListener): void;
  off?(listener: AdapterListener): void;

  start(): Promise<void>;
  stop(): Promise<void>;

  disconnect(deviceId: string): Promise<void>;
  send(deviceId: string, frame: Uint8Array): Promise<void>;
}

export type GatewayEvent =
  | { type: 'connected'; device: Device }
  | { type: 'disconnected'; deviceId: string }
  | { type: 'message'; deviceId: string; message: BridgeToGatewayMsg }
  | { type: 'decodeError'; deviceId: string; description: string };

export type GatewayListener = (event: GatewayEvent) => void;

export class GatewayError extends Error {
  constructor(
    message: string,
    public readonly kind: 'not-running' | 'already-running' | 'request-timed-out' | 'shutdown' | 'send-failed',
  ) {
    super(message);
    this.name = 'GatewayError';
  }
}

type PendingRequest = {
  resolve: (msg: BridgeToGatewayMsg) => void;
  reject: (err: Error) => void;
  timeout: ReturnType<typeof setTimeout>;
};

export type GatewayOptions = {
  logLevel?: LogLevel;
  codec?: Codec;
};

const DEFAULT_REQUEST_TIMEOUT_MS = 30_000;

/**
 * Typed phone-side facade over an `Adapter`.
 *
 * Owns one `FrameAccumulator` per connected device, decodes incoming frames
 * into `BridgeToGatewayMsg`, encodes outbound `GatewayToBridgeMsg` through the
 * shared `Codec`, and tracks in-flight requests so callers can `await` a
 * matching response by id.
 */
export class BridgethingGateway {
  private readonly logger: Logger;
  private readonly codec: Codec;
  private readonly buffers: Map<string, FrameAccumulator> = new Map();
  private readonly pending: Map<string, PendingRequest> = new Map();
  private readonly listeners: Set<GatewayListener> = new Set();
  private readonly adapterListener: AdapterListener;
  private running = false;

  constructor(
    private readonly adapter: Adapter,
    options: GatewayOptions = {},
  ) {
    this.logger = new Logger('Gateway', options.logLevel ?? LogLevel.Log);
    this.codec = options.codec ?? new Codec();
    this.adapterListener = event => this.handleAdapterEvent(event);
  }

  on(listener: GatewayListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  off(listener: GatewayListener): void {
    this.listeners.delete(listener);
  }

  async start(): Promise<void> {
    if (this.running) throw new GatewayError('gateway already started', 'already-running');
    this.running = true;
    this.adapter.on(this.adapterListener);
    await this.adapter.start();
  }

  async stop(): Promise<void> {
    if (!this.running) return;
    this.running = false;
    this.adapter.off?.(this.adapterListener);
    await this.adapter.stop();

    const shutdown = new GatewayError('gateway shutting down', 'shutdown');
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timeout);
      pending.reject(shutdown);
    }
    this.pending.clear();
    this.buffers.clear();
  }

  disconnect(deviceId: string): Promise<void> {
    return this.adapter.disconnect(deviceId);
  }

  /**
   * Encode and ship a fully-formed message. Caller is responsible for picking
   * `meta` (`command`, `event`, etc.). For request/response, prefer `request`
   * which handles id generation and awaiting the matching response.
   *
   * `priority` is a transport-level scheduling hint - Bulk yields to Normal at
   * frame boundaries so latency-sensitive traffic (NowPlaying deltas, like
   * taps) interleaves between long bulk transfers (file/OTA chunks). Default
   * is `'normal'`.
   */
  async send(deviceId: string, message: GatewayToBridgeMsg, options: { priority?: Priority } = {}): Promise<void> {
    const frame = this.codec.encode(message, { priority: options.priority });
    this.logger.trace('send', deviceId, message, options.priority ?? 'normal');
    await this.adapter.send(deviceId, frame);
  }

  /** Bulk-priority shorthand for `send`. */
  async sendBulk(deviceId: string, message: GatewayToBridgeMsg): Promise<void> {
    await this.send(deviceId, message, { priority: 'bulk' });
  }

  /**
   * Send a request and await the matching response by id. The wire id is
   * generated here and matched against
   * `BridgeToGatewayMsg.meta.data.requestId` on the way back.
   */
  request(
    deviceId: string,
    data: GatewayToBridgeMsgData,
    timeoutMs: number = DEFAULT_REQUEST_TIMEOUT_MS,
  ): Promise<BridgeToGatewayMsg> {
    const id = newUuidBytes();
    const key = bytesKey(id);
    const message: GatewayToBridgeMsg = { id, meta: { kind: 'request' }, data };

    return new Promise<BridgeToGatewayMsg>((resolve, reject) => {
      const timeout = setTimeout(() => {
        if (this.pending.delete(key)) {
          reject(new GatewayError(`request ${key} timed out`, 'request-timed-out'));
        }
      }, timeoutMs);
      this.pending.set(key, { resolve, reject, timeout });

      this.send(deviceId, message).catch(err => {
        if (this.pending.delete(key)) {
          clearTimeout(timeout);
          reject(err);
        }
      });
    });
  }

  private handleAdapterEvent(event: AdapterEvent): void {
    switch (event.type) {
      case 'connected':
        this.buffers.set(event.device.id, new FrameAccumulator());
        this.emit({ type: 'connected', device: event.device });
        break;
      case 'disconnected':
        this.buffers.delete(event.deviceId);
        this.emit({ type: 'disconnected', deviceId: event.deviceId });
        break;
      case 'bytes':
        this.ingest(event.deviceId, event.data);
        break;
    }
  }

  private ingest(deviceId: string, chunk: Uint8Array): void {
    let acc = this.buffers.get(deviceId);
    if (!acc) {
      acc = new FrameAccumulator();
      this.buffers.set(deviceId, acc);
    }
    acc.append(chunk);
    while (true) {
      let frame: Uint8Array | null;
      try {
        frame = acc.nextFrame();
      } catch (err) {
        // Lost framing - wipe the accumulator so subsequent bytes have a chance
        // to resync if a new (well-framed) message lands at the next chunk.
        this.buffers.set(deviceId, new FrameAccumulator());
        this.emit({ type: 'decodeError', deviceId, description: errorMessage(err) });
        return;
      }
      if (!frame) return;

      let msg: BridgeToGatewayMsg;
      try {
        msg = this.codec.decode<BridgeToGatewayMsg>(frame);
      } catch (err) {
        this.emit({ type: 'decodeError', deviceId, description: errorMessage(err) });
        continue;
      }

      if (msg.meta.kind === 'response' && this.completePending(msg.meta.data.requestId, msg)) {
        continue;
      }
      this.emit({ type: 'message', deviceId, message: msg });
    }
  }

  private completePending(requestId: Uint8Array, msg: BridgeToGatewayMsg): boolean {
    const key = bytesKey(requestId);
    const pending = this.pending.get(key);
    if (!pending) return false;
    this.pending.delete(key);
    clearTimeout(pending.timeout);
    pending.resolve(msg);
    return true;
  }

  private emit(event: GatewayEvent): void {
    this.logger.trace('event', event.type, event);
    for (const listener of this.listeners) {
      try {
        listener(event);
      } catch (err) {
        this.logger.error('gateway listener threw', err);
      }
    }
  }
}

function bytesKey(bytes: Uint8Array): string {
  let s = '';
  for (let i = 0; i < bytes.length; i++) {
    const h = bytes[i].toString(16);
    s += h.length === 1 ? '0' + h : h;
  }
  return s;
}

function errorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  return String(err);
}

const GATEWAY_VERSION = `v${version}`;

export { GATEWAY_VERSION };
