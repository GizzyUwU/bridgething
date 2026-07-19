import type { Adapter, AdapterEvent, AdapterListener, Device } from '@bridgething/gateway';
import { Logger, LogVerbosity } from '@bridgething/lib';

import { StaticDiscoverer, type Discoverer, type DiscoveryListener, type Endpoint } from './discovery.js';

export { StaticDiscoverer, type Discoverer, type DiscoveryListener, type Endpoint };

const DEFAULT_RECONNECT_BACKOFF_MS = [500, 1_000, 2_000, 4_000, 8_000, 15_000];

export type NetworkAdapterOptions = {
  /**
   * Where to look for daemons. Pass a string URL or an `Endpoint` for the
   * common single-device case, an array for multiple known hosts, or a
   * `Discoverer` for mDNS / custom strategies.
   */
  discovery?: string | Endpoint | Endpoint[] | Discoverer;

  /** Constructor for the underlying WebSocket. Defaults to the global. */
  websocket?: typeof WebSocket;

  /**
   * Auto-reconnect dropped peers with exponential backoff. The default
   * ladder is `[500, 1_000, 2_000, 4_000, 8_000, 15_000]` ms; the last
   * entry repeats. Set to `false` to disable.
   */
  reconnect?: boolean | number[];

  logLevel?: LogVerbosity;
};

/**
 * Byte-level transport for `@bridgething/gateway` over the daemon's network gateway WebSocket.
 * One instance can hold many peers; each `Endpoint` becomes a `Device` whose `id` is the
 * gateway-visible `deviceId`. Works wherever a `WebSocket` exists.
 */
export class NetworkAdapter implements Adapter {
  private readonly logger: Logger;
  private readonly listeners: Set<AdapterListener> = new Set();
  private readonly peers: Map<string, Peer> = new Map();
  private readonly websocketCtor: typeof WebSocket;
  private readonly reconnectLadder: number[] | null;
  private readonly discoverer: Discoverer;
  private running = false;

  constructor(options: NetworkAdapterOptions = {}) {
    this.logger = new Logger('NetworkAdapter', options.logLevel ?? LogVerbosity.Log);
    this.websocketCtor =
      options.websocket ?? (typeof WebSocket !== 'undefined' ? WebSocket : (undefined as unknown as typeof WebSocket));
    if (!this.websocketCtor) {
      throw new Error(
        'NetworkAdapter: no global WebSocket. Pass `options.websocket` (e.g. import { WebSocket } from "ws" on older Node).',
      );
    }
    this.reconnectLadder =
      options.reconnect === false
        ? null
        : Array.isArray(options.reconnect)
          ? options.reconnect
          : DEFAULT_RECONNECT_BACKOFF_MS;
    this.discoverer = resolveDiscoverer(options.discovery);
  }

  on(listener: AdapterListener): void {
    this.listeners.add(listener);
  }

  off(listener: AdapterListener): void {
    this.listeners.delete(listener);
  }

  async start(): Promise<void> {
    if (this.running) return;
    this.running = true;
    await this.discoverer.start(event => this.handleDiscovery(event));
  }

  async stop(): Promise<void> {
    if (!this.running) return;
    this.running = false;
    await this.discoverer.stop();
    const peers = Array.from(this.peers.values());
    this.peers.clear();
    for (const peer of peers) peer.shutdown('adapter-stop');
  }

  disconnect(deviceId: string): Promise<void> {
    const peer = this.peers.get(deviceId);
    if (!peer) return Promise.resolve();
    this.peers.delete(deviceId);
    peer.shutdown('client-disconnect');
    return Promise.resolve();
  }

  send(deviceId: string, frame: Uint8Array): Promise<void> {
    const peer = this.peers.get(deviceId);
    if (!peer) return Promise.reject(new Error(`network-adapter: no active peer for ${deviceId}`));
    peer.send(frame);
    return Promise.resolve();
  }

  private handleDiscovery(event: DiscoveryListener): void {
    if (event.type === 'found') {
      const existing = this.peers.get(event.endpoint.id);
      if (existing) {
        existing.updateEndpoint(event.endpoint);
        return;
      }
      const peer = new Peer({
        endpoint: event.endpoint,
        websocketCtor: this.websocketCtor,
        reconnectLadder: this.reconnectLadder,
        logger: this.logger,
        emit: e => this.emit(e),
      });
      this.peers.set(event.endpoint.id, peer);
      peer.start();
      return;
    }
    const peer = this.peers.get(event.id);
    if (!peer) return;
    this.peers.delete(event.id);
    peer.shutdown('discoverer-lost');
  }

  private emit(event: AdapterEvent): void {
    for (const listener of this.listeners) {
      try {
        listener(event);
      } catch (err) {
        this.logger.error('listener threw', err);
      }
    }
  }
}

function resolveDiscoverer(input: NetworkAdapterOptions['discovery']): Discoverer {
  if (!input) return new StaticDiscoverer({ url: defaultUrl() });
  if (typeof input === 'string') return new StaticDiscoverer({ url: input });
  if (Array.isArray(input)) return new MultiStaticDiscoverer(input);
  if (isDiscoverer(input)) return input;
  return new StaticDiscoverer({ url: input.url, id: input.id, name: input.name });
}

function isDiscoverer(value: object): value is Discoverer {
  return (
    'start' in value &&
    typeof (value as Discoverer).start === 'function' &&
    'stop' in value &&
    typeof (value as Discoverer).stop === 'function'
  );
}

function defaultUrl(): string {
  return 'ws://bridgething.local:8892/';
}

class MultiStaticDiscoverer implements Discoverer {
  private readonly endpoints: Endpoint[];
  private started = false;

  constructor(endpoints: Endpoint[]) {
    this.endpoints = endpoints;
  }

  start(listener: (event: DiscoveryListener) => void): Promise<void> {
    if (this.started) return Promise.resolve();
    this.started = true;
    for (const endpoint of this.endpoints) {
      listener({ type: 'found', endpoint });
    }
    return Promise.resolve();
  }

  stop(): Promise<void> {
    this.started = false;
    return Promise.resolve();
  }
}

type PeerOptions = {
  endpoint: Endpoint;
  websocketCtor: typeof WebSocket;
  reconnectLadder: number[] | null;
  logger: Logger;
  emit: (event: AdapterEvent) => void;
};

class Peer {
  private endpoint: Endpoint;
  private readonly websocketCtor: typeof WebSocket;
  private readonly reconnectLadder: number[] | null;
  private readonly logger: Logger;
  private readonly emit: (event: AdapterEvent) => void;

  private socket: WebSocket | null = null;
  private connected = false;
  private intentionalClose = false;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private reconnectAttempt = 0;
  private sendQueue: Uint8Array[] = [];

  constructor(opts: PeerOptions) {
    this.endpoint = opts.endpoint;
    this.websocketCtor = opts.websocketCtor;
    this.reconnectLadder = opts.reconnectLadder;
    this.logger = opts.logger;
    this.emit = opts.emit;
  }

  start(): void {
    this.openSocket();
  }

  updateEndpoint(next: Endpoint): void {
    const urlChanged = next.url !== this.endpoint.url;
    this.endpoint = next;
    if (!urlChanged) return;
    this.logger.log(`endpoint ${next.id} url changed to ${next.url}, reconnecting`);
    this.reconnectSoon();
  }

  send(frame: Uint8Array): void {
    if (this.socket && this.connected) {
      try {
        this.socket.send(toArrayBufferSlice(frame));
      } catch (err) {
        this.logger.warn(`send to ${this.endpoint.id} failed, queueing`, err);
        this.sendQueue.push(frame);
        this.reconnectSoon();
      }
      return;
    }
    this.sendQueue.push(frame);
  }

  shutdown(reason: string): void {
    this.intentionalClose = true;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.sendQueue = [];
    const socket = this.socket;
    this.socket = null;
    if (socket) {
      try {
        socket.close(1000, reason);
      } catch {
        // best-effort
      }
    }
    if (this.connected) {
      this.connected = false;
      this.emit({ type: 'disconnected', deviceId: this.endpoint.id });
    }
  }

  private openSocket(): void {
    if (this.intentionalClose) return;
    let socket: WebSocket;
    try {
      socket = new this.websocketCtor(this.endpoint.url);
    } catch (err) {
      this.logger.warn(`failed to construct websocket for ${this.endpoint.url}`, err);
      this.scheduleReconnect();
      return;
    }
    socket.binaryType = 'arraybuffer';
    this.socket = socket;

    socket.addEventListener('open', () => {
      this.connected = true;
      this.reconnectAttempt = 0;
      this.emit({
        type: 'connected',
        device: this.deviceForEndpoint(),
      });
      this.flushQueue();
    });

    socket.addEventListener('message', event => this.handleMessage(event.data));

    socket.addEventListener('close', () => {
      this.socket = null;
      if (this.connected) {
        this.connected = false;
        this.emit({ type: 'disconnected', deviceId: this.endpoint.id });
      }
      if (!this.intentionalClose) this.scheduleReconnect();
    });

    socket.addEventListener('error', () => {
      this.logger.trace(`ws error on ${this.endpoint.id}`);
    });
  }

  private handleMessage(raw: unknown): void {
    let bytes: Uint8Array | null = null;
    if (raw instanceof ArrayBuffer) bytes = new Uint8Array(raw);
    else if (raw instanceof Uint8Array) bytes = raw;
    else if (ArrayBuffer.isView(raw)) {
      const view = raw;
      bytes = new Uint8Array(view.buffer, view.byteOffset, view.byteLength);
    } else if (typeof raw === 'string') {
      this.logger.warn(`unexpected text frame from ${this.endpoint.id}; network gateway is binary-only`);
      return;
    } else if (raw && typeof (raw as { arrayBuffer?: () => Promise<ArrayBuffer> }).arrayBuffer === 'function') {
      void (raw as { arrayBuffer: () => Promise<ArrayBuffer> })
        .arrayBuffer()
        .then(buf => this.emit({ type: 'bytes', deviceId: this.endpoint.id, data: new Uint8Array(buf) }))
        .catch(err => this.logger.warn(`failed to read Blob frame from ${this.endpoint.id}`, err));
      return;
    }

    if (!bytes || bytes.byteLength === 0) return;
    this.emit({ type: 'bytes', deviceId: this.endpoint.id, data: bytes });
  }

  private flushQueue(): void {
    const queued = this.sendQueue;
    this.sendQueue = [];
    for (const frame of queued) {
      if (!this.socket || !this.connected) {
        this.sendQueue.push(frame);
        continue;
      }
      try {
        this.socket.send(toArrayBufferSlice(frame));
      } catch (err) {
        this.logger.warn(`flush send to ${this.endpoint.id} failed`, err);
        this.sendQueue.push(frame);
        this.reconnectSoon();
        return;
      }
    }
  }

  private reconnectSoon(): void {
    if (this.intentionalClose) return;
    const socket = this.socket;
    this.socket = null;
    if (socket) {
      try {
        socket.close(1000, 'reconnect');
      } catch {
        // best-effort
      }
    }
    if (this.connected) {
      this.connected = false;
      this.emit({ type: 'disconnected', deviceId: this.endpoint.id });
    }
    this.scheduleReconnect();
  }

  private scheduleReconnect(): void {
    if (!this.reconnectLadder) return;
    const idx = Math.min(this.reconnectAttempt, this.reconnectLadder.length - 1);
    const delay = this.reconnectLadder[idx];
    this.reconnectAttempt += 1;
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer);
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.openSocket();
    }, delay);
  }

  private deviceForEndpoint(): Device {
    return {
      id: this.endpoint.id,
      name: this.endpoint.name ?? this.endpoint.url,
    };
  }
}

/**
 * DOM type checking rejects `Uint8Array<ArrayBufferLike>` because the union includes
 * `SharedArrayBuffer`. The codec only produces `ArrayBuffer`-backed views, so this slice is safe.
 */
function toArrayBufferSlice(frame: Uint8Array): ArrayBuffer {
  const buffer = frame.buffer;
  if (buffer instanceof ArrayBuffer && frame.byteOffset === 0 && frame.byteLength === buffer.byteLength) {
    return buffer;
  }
  const out = new ArrayBuffer(frame.byteLength);
  new Uint8Array(out).set(frame);
  return out;
}
