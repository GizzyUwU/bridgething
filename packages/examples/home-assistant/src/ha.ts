import type { BridgethingClient } from '@bridgething/client';

export type HaState = { entityId: string; state: string; attributes: Record<string, unknown> };
export type HaEntities = Record<string, HaState>;

export type HaStatus =
  | { kind: 'connecting' }
  | { kind: 'authenticating' }
  | { kind: 'ready' }
  | { kind: 'error'; message: string };

type HaOpts = {
  client: BridgethingClient;
  baseUrl: string;
  token: string;
  onStatus: (status: HaStatus) => void;
};

type Pending = { resolve: (result: unknown) => void; reject: (err: Error) => void };

// compact entity shape used by subscribe_entities: s = state, a = attributes.
type Compact = { s?: string; a?: Record<string, unknown> };
type EntitiesEvent = {
  a?: Record<string, Compact>;
  c?: Record<string, { '+'?: Compact; '-'?: Compact }>;
  r?: string[];
};

const RECONNECT_MIN = 1_000;
const RECONNECT_MAX = 20_000;

/**
 * One Home Assistant WebSocket session, tunneled through the daemon's
 * net.ws proxy on the connected phone. Owns the auth handshake, the
 * id-keyed request/response map, and a single subscribe_entities stream.
 * Reconnects with backoff and replays the active subscription.
 */
export class HaConnection {
  private readonly client: BridgethingClient;
  private readonly wsUrl: string;
  private readonly token: string;
  private readonly onStatus: (status: HaStatus) => void;

  private connectionId = '';
  private nextId = 1;
  private readonly pending = new Map<number, Pending>();
  private offFns: Array<() => void> = [];

  private entities: HaEntities = {};
  private subscriptionId: number | null = null;
  private subscribedIds: string[] = [];
  private onEntities: ((entities: HaEntities) => void) | null = null;

  private ready: Promise<void>;
  private resolveReady: (() => void) | null = null;
  private rejectReady: ((err: Error) => void) | null = null;
  private reconnectDelay = RECONNECT_MIN;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private closed = false;

  constructor(opts: HaOpts) {
    this.client = opts.client;
    this.token = opts.token;
    this.onStatus = opts.onStatus;
    this.wsUrl = toWsUrl(opts.baseUrl);
    this.ready = this.freshReady();
  }

  /** Resolves on the first successful auth_ok; rejects on auth_invalid or a hard open failure. */
  whenReady(): Promise<void> {
    return this.ready;
  }

  async open(): Promise<void> {
    this.closed = false;
    await this.dial();
  }

  close(): void {
    this.closed = true;
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer);
    this.teardownSocket();
  }

  /** One-shot full entity list, for the picker. */
  async getStates(): Promise<HaState[]> {
    await this.ready;
    const result = (await this.request({ type: 'get_states' })) as Array<{
      entity_id: string;
      state: string;
      attributes: Record<string, unknown>;
    }>;
    return result.map(s => ({ entityId: s.entity_id, state: s.state, attributes: s.attributes }));
  }

  /** Subscribe to live updates for exactly `entityIds`. Replaces any prior subscription. */
  async subscribeEntities(entityIds: string[], onEntities: (entities: HaEntities) => void): Promise<void> {
    await this.ready;
    this.onEntities = onEntities;
    this.subscribedIds = [...entityIds];
    await this.startSubscription();
  }

  async callService(
    domain: string,
    service: string,
    data: Record<string, unknown>,
    target: Record<string, unknown>,
  ): Promise<void> {
    await this.ready;
    await this.request({ type: 'call_service', domain, service, service_data: data, target });
  }

  private freshReady(): Promise<void> {
    return new Promise<void>((resolve, reject) => {
      this.resolveReady = resolve;
      this.rejectReady = reject;
    });
  }

  private async dial(): Promise<void> {
    this.connectionId = crypto.randomUUID();
    this.installListeners();
    this.onStatus({ kind: 'connecting' });
    const res = await this.client.net.wsOpen({
      connectionId: this.connectionId,
      url: this.wsUrl,
      protocols: null,
      headers: null,
    });
    if (!res.ok) {
      this.onStatus({ kind: 'error', message: openErrorMessage(res) });
      this.scheduleReconnect();
      return;
    }
    this.onStatus({ kind: 'authenticating' });
  }

  private installListeners(): void {
    this.teardownListeners();
    this.offFns = [
      this.client.net.onWsMessage(msg => {
        if (msg.connectionId !== this.connectionId || msg.frame.type !== 'text') return;
        this.handle(JSON.parse(msg.frame.data));
      }),
      this.client.net.onWsClosed(msg => {
        if (msg.connectionId !== this.connectionId) return;
        this.onLost('home assistant closed the connection');
      }),
      this.client.net.onWsErrorEvent(msg => {
        if (msg.connectionId !== this.connectionId) return;
        this.onLost('network error on the home assistant link');
      }),
    ];
  }

  private handle(msg: { type: string; id?: number; [k: string]: unknown }): void {
    switch (msg.type) {
      case 'auth_required':
        void this.send({ type: 'auth', access_token: this.token });
        return;
      case 'auth_ok':
        this.reconnectDelay = RECONNECT_MIN;
        this.onStatus({ kind: 'ready' });
        this.resolveReady?.();
        if (this.subscribedIds.length) void this.startSubscription();
        return;
      case 'auth_invalid':
        this.onStatus({ kind: 'error', message: 'home assistant rejected the token; re-check it in the companion app.' });
        this.rejectReady?.(new Error('auth_invalid'));
        this.closed = true;
        this.teardownSocket();
        return;
      case 'result': {
        const p = msg.id != null ? this.pending.get(msg.id) : undefined;
        if (!p) return;
        this.pending.delete(msg.id!);
        if (msg['success']) p.resolve((msg as { result?: unknown }).result);
        else p.reject(new Error(serviceError(msg)));
        return;
      }
      case 'event':
        if (msg.id === this.subscriptionId) this.applyEvent((msg as unknown as { event: EntitiesEvent }).event);
        return;
      default:
        return;
    }
  }

  private async startSubscription(): Promise<void> {
    if (this.subscriptionId != null) {
      const old = this.subscriptionId;
      this.subscriptionId = null;
      this.request({ type: 'unsubscribe_events', subscription: old }).catch(() => {});
    }
    this.entities = {};
    const id = this.nextId;
    this.subscriptionId = id;
    this.request({ type: 'subscribe_entities', entity_ids: this.subscribedIds }, id).catch(() => {});
  }

  private applyEvent(ev: EntitiesEvent): void {
    if (ev.a) for (const [id, c] of Object.entries(ev.a)) this.entities[id] = fromCompact(id, c);
    if (ev.c)
      for (const [id, diff] of Object.entries(ev.c)) {
        const cur = this.entities[id];
        if (!cur) continue;
        const plus = diff['+'];
        if (plus) {
          if (plus.s !== undefined) cur.state = plus.s;
          if (plus.a) cur.attributes = { ...cur.attributes, ...plus.a };
        }
        const minus = diff['-'];
        if (minus?.a) for (const k of Object.keys(minus.a)) delete cur.attributes[k];
      }
    if (ev.r) for (const id of ev.r) delete this.entities[id];
    this.onEntities?.({ ...this.entities });
  }

  private request(payload: Record<string, unknown>, forcedId?: number): Promise<unknown> {
    const id = forcedId ?? this.nextId;
    this.nextId = Math.max(this.nextId, id) + 1;
    return new Promise<unknown>((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.send({ ...payload, id }).catch(reject);
    });
  }

  private async send(payload: Record<string, unknown>): Promise<void> {
    await this.client.net.wsSend({ connectionId: this.connectionId, frame: { type: 'text', data: JSON.stringify(payload) } });
  }

  private onLost(message: string): void {
    if (this.closed) return;
    this.onStatus({ kind: 'error', message });
    this.scheduleReconnect();
  }

  private scheduleReconnect(): void {
    if (this.closed || this.reconnectTimer) return;
    this.teardownSocket();
    const delay = this.reconnectDelay;
    this.reconnectDelay = Math.min(this.reconnectDelay * 2, RECONNECT_MAX);
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      void this.dial();
    }, delay);
  }

  private teardownSocket(): void {
    this.teardownListeners();
    for (const p of this.pending.values()) p.reject(new Error('connection closed'));
    this.pending.clear();
    this.subscriptionId = null;
    if (this.connectionId) {
      this.client.net.wsClose({ connectionId: this.connectionId, code: null, reason: null }).catch(() => {});
      this.connectionId = '';
    }
  }

  private teardownListeners(): void {
    for (const off of this.offFns) off();
    this.offFns = [];
  }
}

function fromCompact(entityId: string, c: Compact): HaState {
  return { entityId, state: c.s ?? 'unknown', attributes: c.a ?? {} };
}

function toWsUrl(baseUrl: string): string {
  const trimmed = baseUrl.trim().replace(/\/+$/, '');
  const withScheme = /^https?:\/\//i.test(trimmed) ? trimmed : `https://${trimmed}`;
  const ws = withScheme.replace(/^http/i, 'ws');
  return `${ws}/api/websocket`;
}

function openErrorMessage(res: { kind: 'domain' | 'protocol'; error: unknown }): string {
  if (res.kind === 'domain') {
    const err = res.error as { type?: string };
    if (err.type === 'connectFailed') return 'could not reach home assistant; check the URL and that the phone has network.';
    if (err.type === 'gatewayDisconnected') return 'no network - is a phone connected to the companion app?';
  }
  return 'could not open the home assistant connection.';
}

function serviceError(msg: { [k: string]: unknown }): string {
  const err = msg['error'] as { message?: string } | undefined;
  return err?.message ?? 'home assistant rejected the request';
}
