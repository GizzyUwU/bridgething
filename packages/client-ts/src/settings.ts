import type { ConfigEntry, ConfigField, DocEntry } from '@bridgething/lib';

export type { ConfigEntry, ConfigField, DocEntry } from '@bridgething/lib';

const CALL_TIMEOUT_MS = 15_000;
const HOST_MISSING = 'not running inside the bridgething companion settings host (window.ReactNativeWebView is absent)';

/** Identity of the webapp whose settings page is being rendered. */
export type SettingsContext = { webappId: string; name: string; version: string; deviceId: string };

/** A config or doc key/value after a write; `value` is null when reset to no value. */
export type KeyValue = { key: string; value: string | null };

/** Callback for unsolicited doc changes pushed by the host. */
export type DocChangedListener = (key: string, value: string | null) => void;

type HostReply = { id: number; ok: true; value: unknown } | { id: number; ok: false; error: string };
type HostEvent = { event: 'docChanged'; key: string; value: string | null };

type Pending = {
  resolve: (value: unknown) => void;
  reject: (err: Error) => void;
  timer: ReturnType<typeof setTimeout>;
};

declare global {
  interface Window {
    ReactNativeWebView?: { postMessage: (json: string) => void };
    __bridgethingSettingsDeliver?: (json: string) => void;
  }
}

let nextId = 1;
const pending = new Map<number, Pending>();
const docListeners = new Set<DocChangedListener>();

function deliver(json: string): void {
  let parsed: HostReply | HostEvent;
  try {
    parsed = JSON.parse(json) as HostReply | HostEvent;
  } catch {
    return;
  }

  if ('event' in parsed && parsed.event === 'docChanged') {
    for (const listener of docListeners) listener(parsed.key, parsed.value);
    return;
  }

  const reply = parsed as HostReply;
  const entry = pending.get(reply.id);
  if (!entry) return;
  pending.delete(reply.id);
  clearTimeout(entry.timer);
  if (reply.ok) entry.resolve(reply.value);
  else entry.reject(new Error(reply.error));
}

if (typeof window !== 'undefined') window.__bridgethingSettingsDeliver = deliver;

function call<T>(verb: string, payload?: unknown): Promise<T> {
  const host = typeof window !== 'undefined' ? window.ReactNativeWebView : undefined;
  if (!host) return Promise.reject(new Error(HOST_MISSING));

  const id = nextId++;
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => {
      if (pending.delete(id))
        reject(new Error(`bridgething settings call '${verb}' timed out after ${CALL_TIMEOUT_MS}ms`));
    }, CALL_TIMEOUT_MS);
    pending.set(id, { resolve: resolve as (value: unknown) => void, reject, timer });
    try {
      host.postMessage(JSON.stringify({ id, verb, payload }));
    } catch (err) {
      pending.delete(id);
      clearTimeout(timer);
      reject(err instanceof Error ? err : new Error(String(err)));
    }
  });
}

/**
 * Bridge a webapp settings page to its host inside the bridgething companion
 * app. Speaks the React-Native WebView `postMessage` protocol, not a
 * WebSocket; the host delivers replies through a global installed at import.
 *
 * Network caveat: the page runs on the phone with real internet, but from a
 * `file://` origin, so the webview enforces CORS on fetch/XHR (requests carry
 * `Origin: null`). Websocket APIs are not CORS-gated and work out of the box;
 * prefer them. For a CORS-strict HTTP-only service, fetch from the on-device
 * webapp via `client.net` (phone-tunneled, not origin-restricted) instead.
 */
export const settings = {
  /** Identity of the webapp and device this settings page is scoped to. */
  context(): Promise<SettingsContext> {
    return call<SettingsContext>('context');
  },
  config: {
    /** The config schema the webapp declared in its manifest. */
    fields(): Promise<ConfigField[]> {
      return call<ConfigField[]>('config.fields');
    },
    /** Current config values (declared defaults plus user overrides). */
    list(): Promise<ConfigEntry[]> {
      return call<ConfigEntry[]>('config.list');
    },
    /** Set a config value; resolves with the stored key/value. */
    set(key: string, value: string): Promise<KeyValue> {
      return call<KeyValue>('config.set', { key, value });
    },
    /** Clear a config override; resolves with the restored default or null. */
    delete(key: string): Promise<KeyValue> {
      return call<KeyValue>('config.delete', { key });
    },
  },
  doc: {
    /** Read one doc value; resolves with null when unset. */
    get(key: string): Promise<KeyValue> {
      return call<KeyValue>('doc.get', { key });
    },
    /** List every doc key/value in the webapp's namespace. */
    list(): Promise<DocEntry[]> {
      return call<DocEntry[]>('doc.list');
    },
    /** Write one doc value, readable by both the webapp and companion. */
    set(key: string, value: string): Promise<KeyValue> {
      return call<KeyValue>('doc.set', { key, value });
    },
    /** Delete one doc value; resolves with a null value. */
    delete(key: string): Promise<{ key: string; value: null }> {
      return call<{ key: string; value: null }>('doc.delete', { key });
    },
  },
  /** Subscribe to host-pushed doc changes; returns an unsubscribe function. */
  onDocChanged(listener: DocChangedListener): () => void {
    docListeners.add(listener);
    return () => docListeners.delete(listener);
  },
  /** Signal the host to close the settings webview. Fire-and-forget. */
  done(): void {
    const host = typeof window !== 'undefined' ? window.ReactNativeWebView : undefined;
    if (!host) throw new Error(HOST_MISSING);
    host.postMessage(JSON.stringify({ id: nextId++, verb: 'done' }));
  },
};
