import {
  type BridgeToClientMsg,
  type ClientToBridgeMsg,
  type ClientToBridgeMsgData,
  Logger,
  LogLevel,
  newUuidBytes,
} from '@bridgething/lib';

const DEFAULT_URL = 'ws://127.0.0.1:8891/';
const DEFAULT_REQUEST_TIMEOUT_MS = 30_000;
const RECONNECT_BACKOFF_MS = [500, 1_000, 2_000, 4_000, 8_000, 15_000];

export type ConnectionState = 'connecting' | 'open' | 'closing' | 'closed';

export type ClientEvent =
  | { type: 'open' }
  | { type: 'close'; code?: number; reason?: string }
  | { type: 'connecting'; attempt: number }
  | { type: 'message'; message: BridgeToClientMsg }
  | { type: 'decodeError'; description: string };

export type ClientListener = (event: ClientEvent) => void;

export class ClientError extends Error {
  constructor(
    message: string,
    public readonly kind:
      | 'not-running'
      | 'already-running'
      | 'request-timed-out'
      | 'shutdown'
      | 'send-failed'
      | 'not-connected',
    err?: unknown,
  ) {
    super(message);
    this.name = 'ClientError';
    if (err instanceof Error && err.stack) this.stack = `${this.name}: ${this.message}\nCaused by: ${err.stack}`;
  }
}

type PendingRequest = {
  resolve: (msg: BridgeToClientMsg) => void;
  reject: (err: Error) => void;
  timeout: ReturnType<typeof setTimeout>;
};

export type ClientOptions = {
  /** WebSocket URL. Defaults to `ws://127.0.0.1:8891/` (the on-device daemon). */
  url?: string;
  logLevel?: LogLevel;
  /** Auto-connect on construct. Defaults to `true`. */
  autoConnect?: boolean;
  /** Auto-reconnect on close with exponential backoff. Defaults to `true`. */
  reconnect?: boolean;
  /** Constructor for the underlying WebSocket. Defaults to the global `WebSocket` (browser/runtime). */
  websocket?: typeof WebSocket;
};

/**
 * Typed webapp-side facade over the on-device daemon's local WebSocket.
 *
 * Single-peer (the daemon), so no device proxy or broadcast machinery —
 * every method addresses the daemon implicitly. Holds one `WebSocket`
 * with auto-reconnect, JSON-encodes outbound `ClientToBridgeMsg`,
 * decodes inbound `BridgeToClientMsg`, and tracks in-flight requests so
 * callers can `await` a matching response by id.
 *
 * Surface methods (`client.player.onState`, `client.storage.get`, etc.)
 * are emitted by codegen into `dispatch.generated.ts` and applied to the
 * prototype at module-load via `applyDispatch()`.
 */
export class BridgethingClient {
  public readonly logger: Logger;
  public readonly url: string;
  private readonly listeners: Set<ClientListener> = new Set();
  private readonly pending: Map<string, PendingRequest> = new Map();
  private readonly websocketCtor: typeof WebSocket;
  private readonly reconnectEnabled: boolean;

  private socket: WebSocket | null = null;
  private state: ConnectionState = 'closed';
  private reconnectAttempt = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private intentionalClose = false;

  constructor(options: ClientOptions = {}) {
    this.logger = new Logger('Client', options.logLevel ?? LogLevel.Log);
    this.url = options.url ?? DEFAULT_URL;
    this.reconnectEnabled = options.reconnect ?? true;
    this.websocketCtor =
      options.websocket ?? (typeof WebSocket !== 'undefined' ? WebSocket : (undefined as unknown as typeof WebSocket));

    if (!this.websocketCtor) {
      throw new ClientError(
        'no global WebSocket available; pass `options.websocket` (e.g. from `ws` on Node)',
        'not-running',
      );
    }

    if (options.autoConnect ?? true) this.connect();
  }

  get connectionState(): ConnectionState {
    return this.state;
  }

  on(listener: ClientListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  off(listener: ClientListener): void {
    this.listeners.delete(listener);
  }

  /**
   * Open the WebSocket if it is not already open. Idempotent: if the
   * socket is already connecting/open, returns the same in-flight promise.
   */
  connect(): void {
    if (this.state === 'open' || this.state === 'connecting') return;
    this.intentionalClose = false;
    this.openSocket();
  }

  /**
   * Close the WebSocket and cancel any reconnect timer. Pending requests
   * are rejected with `'shutdown'`. After `close()`, `connect()` reopens.
   */
  close(): void {
    this.intentionalClose = true;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    if (this.socket && (this.state === 'open' || this.state === 'connecting')) {
      this.state = 'closing';
      this.socket.close();
    }
    const shutdown = new ClientError('client shutting down', 'shutdown');
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timeout);
      pending.reject(shutdown);
    }
    this.pending.clear();
  }

  /**
   * Encode and ship a fully-formed message. Caller is responsible for
   * picking `meta` (`command`, `event`, etc.). For request/response,
   * prefer `request` or one of the codegen-emitted typed query methods.
   */
  async send(message: ClientToBridgeMsg): Promise<void> {
    if (!this.socket || this.state !== 'open') {
      throw new ClientError('client not connected', 'not-connected');
    }
    this.logger.trace('send', message);
    try {
      this.socket.send(JSON.stringify(serialize(message)));
    } catch (err) {
      throw new ClientError('failed to send message', 'send-failed', err);
    }
  }

  /**
   * Send a request and await the matching response by id. The wire id
   * is generated here and matched against `BridgeToClientMsg.meta.data.requestId`
   * on the way back.
   */
  request(data: ClientToBridgeMsgData, timeoutMs: number = DEFAULT_REQUEST_TIMEOUT_MS): Promise<BridgeToClientMsg> {
    const id = newUuidBytes();
    const key = bytesKey(id);
    const message: ClientToBridgeMsg = { id, meta: { kind: 'request' }, data };

    return new Promise<BridgeToClientMsg>((resolve, reject) => {
      const timeout = setTimeout(() => {
        if (this.pending.delete(key)) {
          reject(new ClientError(`request ${key} timed out`, 'request-timed-out'));
        }
      }, timeoutMs);
      this.pending.set(key, { resolve, reject, timeout });

      this.send(message).catch(err => {
        if (this.pending.delete(key)) {
          clearTimeout(timeout);
          reject(new ClientError(`failed to send request ${key}`, 'send-failed', err));
        }
      });
    });
  }

  private openSocket(): void {
    this.state = 'connecting';
    this.emit({ type: 'connecting', attempt: this.reconnectAttempt });

    let socket: WebSocket;
    try {
      socket = new this.websocketCtor(this.url);
    } catch (err) {
      this.logger.error('failed to construct WebSocket', err);
      this.scheduleReconnect();
      return;
    }
    this.socket = socket;
    socket.binaryType = 'arraybuffer';

    socket.addEventListener('open', () => {
      this.state = 'open';
      this.reconnectAttempt = 0;
      this.emit({ type: 'open' });
    });

    socket.addEventListener('message', event => this.handleMessage(event.data));

    socket.addEventListener('close', event => {
      this.socket = null;
      this.state = 'closed';
      this.emit({ type: 'close', code: event.code, reason: event.reason });
      if (!this.intentionalClose && this.reconnectEnabled) this.scheduleReconnect();
    });

    socket.addEventListener('error', () => {
      this.logger.warn('websocket error event (will close shortly)');
    });
  }

  private scheduleReconnect(): void {
    const idx = Math.min(this.reconnectAttempt, RECONNECT_BACKOFF_MS.length - 1);
    const delay = RECONNECT_BACKOFF_MS[idx];
    this.reconnectAttempt += 1;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.openSocket();
    }, delay);
  }

  private handleMessage(raw: unknown): void {
    let text: string;
    if (typeof raw === 'string') {
      text = raw;
    } else if (raw instanceof ArrayBuffer) {
      text = new TextDecoder().decode(raw);
    } else if (raw && typeof (raw as { toString?: () => string }).toString === 'function') {
      text = String(raw);
    } else {
      this.emit({ type: 'decodeError', description: 'unknown message payload type' });
      return;
    }

    let parsed: unknown;
    try {
      parsed = JSON.parse(text);
    } catch (err) {
      this.emit({ type: 'decodeError', description: errorMessage(err) });
      return;
    }

    let msg: BridgeToClientMsg;
    try {
      msg = deserialize(parsed) as BridgeToClientMsg;
    } catch (err) {
      this.emit({ type: 'decodeError', description: errorMessage(err) });
      return;
    }

    if (msg.meta.kind === 'response' && this.completePending(msg.meta.data.requestId, msg)) {
      return;
    }
    this.emit({ type: 'message', message: msg });
  }

  private completePending(requestId: Uint8Array, msg: BridgeToClientMsg): boolean {
    const key = bytesKey(requestId);
    const pending = this.pending.get(key);
    if (!pending) return false;
    this.pending.delete(key);
    clearTimeout(pending.timeout);
    pending.resolve(msg);
    return true;
  }

  private emit(event: ClientEvent): void {
    this.logger.trace('event', event.type, event);
    for (const listener of this.listeners) {
      try {
        listener(event);
      } catch (err) {
        this.logger.error('client listener threw', err);
      }
    }
  }
}

/**
 * Recursively walk a value and convert `Uint8Array` to a base64 string
 * tagged with a sentinel so the daemon's serde-msgpack-bytes-handling
 * stays compatible with JSON transport. The on-device daemon's WS
 * connection layer expects UUIDs as 16-element byte arrays, so we ship
 * them as plain JSON arrays of integers.
 */
function serialize(value: unknown): unknown {
  if (value instanceof Uint8Array) {
    return Array.from(value);
  }
  if (Array.isArray(value)) {
    return value.map(serialize);
  }
  if (value && typeof value === 'object') {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
      out[k] = serialize(v);
    }
    return out;
  }
  return value;
}

/**
 * Inverse of `serialize`: walk the parsed object and turn any 16-element
 * number array under a known UUID-shaped key back into `Uint8Array`. We
 * only target the well-known UUID-bearing fields (`id`, `requestId`)
 * because there's no general way to distinguish a byte array from a
 * numeric array by inspection.
 */
function deserialize(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(deserialize);
  }
  if (value && typeof value === 'object') {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
      if (
        (k === 'id' || k === 'requestId') &&
        Array.isArray(v) &&
        v.length === 16 &&
        v.every(n => typeof n === 'number')
      ) {
        out[k] = Uint8Array.from(v as number[]);
      } else {
        out[k] = deserialize(v);
      }
    }
    return out;
  }
  return value;
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

export { LogLevel } from '@bridgething/lib';

import { applyDispatch } from './dispatch.generated';
applyDispatch();
