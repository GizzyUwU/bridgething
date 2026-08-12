import type { ConfigEntry, ConfigField, DocEntry } from '@bridgething/lib';

export type { ConfigEntry, ConfigField, DocEntry } from '@bridgething/lib';

const CALL_TIMEOUT_MS = 15_000;
const HOST_MISSING = 'not running inside a bridgething settings host (no companion webview, no host frame)';

export type SettingsContext = { webappId: string; name: string; version: string; deviceId: string };
export type KeyValue = { key: string; value: string | null };
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

function send(json: string): boolean {
  if (typeof window === 'undefined') return false;

  const webview = window.ReactNativeWebView;
  if (webview) {
    webview.postMessage(json);
    return true;
  }

  if (window.parent !== window) {
    window.parent.postMessage(json, '*');
    return true;
  }

  return false;
}

if (typeof window !== 'undefined') {
  window.__bridgethingSettingsDeliver = deliver;
  window.addEventListener('message', event => {
    if (event.source !== window.parent || event.source === window) return;
    if (typeof event.data === 'string') deliver(event.data);
  });
}

function call<T>(verb: string, payload?: unknown): Promise<T> {
  const id = nextId++;
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => {
      if (pending.delete(id))
        reject(new Error(`bridgething settings call '${verb}' timed out after ${CALL_TIMEOUT_MS}ms`));
    }, CALL_TIMEOUT_MS);
    pending.set(id, { resolve: resolve as (value: unknown) => void, reject, timer });
    try {
      if (!send(JSON.stringify({ id, verb, payload }))) throw new Error(HOST_MISSING);
    } catch (err) {
      pending.delete(id);
      clearTimeout(timer);
      reject(err instanceof Error ? err : new Error(String(err)));
    }
  });
}

export const settings = {
  context(): Promise<SettingsContext> {
    return call<SettingsContext>('context');
  },
  config: {
    fields(): Promise<ConfigField[]> {
      return call<ConfigField[]>('config.fields');
    },
    list(): Promise<ConfigEntry[]> {
      return call<ConfigEntry[]>('config.list');
    },
    set(key: string, value: string): Promise<KeyValue> {
      return call<KeyValue>('config.set', { key, value });
    },
    delete(key: string): Promise<KeyValue> {
      return call<KeyValue>('config.delete', { key });
    },
  },
  doc: {
    get(key: string): Promise<KeyValue> {
      return call<KeyValue>('doc.get', { key });
    },
    list(): Promise<DocEntry[]> {
      return call<DocEntry[]>('doc.list');
    },
    set(key: string, value: string): Promise<KeyValue> {
      return call<KeyValue>('doc.set', { key, value });
    },
    delete(key: string): Promise<{ key: string; value: null }> {
      return call<{ key: string; value: null }>('doc.delete', { key });
    },
  },
  onDocChanged(listener: DocChangedListener): () => void {
    docListeners.add(listener);
    return () => docListeners.delete(listener);
  },
  done(): void {
    if (!send(JSON.stringify({ id: nextId++, verb: 'done' }))) throw new Error(HOST_MISSING);
  },
};
