import { BridgethingClient } from '@bridgething/client';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import Dashboard from './Dashboard';
import { controlKind, momentaryCall, optimisticToggle, toggleCall } from './domains';
import { HaConnection, type HaEntities, type HaState, type HaStatus } from './ha';
import Picker from './Picker';

const wsUrl =
  import.meta.env.VITE_BRIDGETHING_URL ??
  (typeof window !== 'undefined' ? `ws://${window.location.host}/` : 'ws://127.0.0.1:8891/');

const SELECTION_KEY = 'selected_entities';
const TEMP_DEBOUNCE_MS = 600;

type Mode =
  | { kind: 'loading' }
  | { kind: 'needs-config'; message: string }
  | { kind: 'picker'; all: HaState[] }
  | { kind: 'dashboard' };

export default function App() {
  const client = useMemo(() => new BridgethingClient({ url: wsUrl }), []);
  const connRef = useRef<HaConnection | null>(null);
  const tempTimers = useRef(new Map<string, ReturnType<typeof setTimeout>>());

  const [mode, setMode] = useState<Mode>({ kind: 'loading' });
  const [status, setStatus] = useState<HaStatus>({ kind: 'connecting' });
  const [selection, setSelection] = useState<string[]>([]);
  const [entities, setEntities] = useState<HaEntities>({});
  const [overlay, setOverlay] = useState<Record<string, string>>({});
  const [pendingTemp, setPendingTemp] = useState<Record<string, number>>({});
  const [toast, setToast] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);

  const flash = useCallback((message: string) => {
    setToast(message);
    setTimeout(() => setToast(t => (t === message ? null : t)), 3_000);
  }, []);

  // live updates reconcile any optimistic overlay that has caught up to reality.
  const ingest = useCallback((next: HaEntities) => {
    setEntities(next);
    setOverlay(prev => {
      let changed = false;
      const out = { ...prev };
      for (const [id, value] of Object.entries(prev)) {
        if (next[id]?.state === value) {
          delete out[id];
          changed = true;
        }
      }
      return changed ? out : prev;
    });
    setPendingTemp(prev => {
      let changed = false;
      const out = { ...prev };
      for (const [id, target] of Object.entries(prev)) {
        if (numAttr(next[id]?.attributes['temperature']) === target) {
          delete out[id];
          changed = true;
        }
      }
      return changed ? out : prev;
    });
  }, []);

  useEffect(() => {
    let cancelled = false;
    const timers = tempTimers.current;

    const boot = async () => {
      setMode({ kind: 'loading' });
      const [baseCfg, tokenCfg] = await Promise.all([
        client.config.get({ key: 'base_url' }),
        client.config.get({ key: 'token' }),
      ]);
      const baseUrl = baseCfg.ok ? baseCfg.response.value : null;
      const token = tokenCfg.ok ? tokenCfg.response.value : null;
      if (!baseUrl || !token) {
        if (!cancelled)
          setMode({ kind: 'needs-config', message: 'set the Home Assistant URL and access token in the companion app.' });
        return;
      }

      const conn = new HaConnection({ client, baseUrl, token, onStatus: s => !cancelled && setStatus(s) });
      connRef.current = conn;
      await conn.open();
      try {
        await conn.whenReady();
      } catch {
        return; // status already carries the error; auth_invalid is terminal
      }
      if (cancelled) return;

      const stored = await client.store.get({ key: SELECTION_KEY });
      const ids = stored.ok && stored.response.value ? splitIds(stored.response.value) : [];
      if (cancelled) return;

      if (ids.length === 0) {
        const all = await conn.getStates();
        if (!cancelled) setMode({ kind: 'picker', all });
      } else {
        setSelection(ids);
        await conn.subscribeEntities(ids, e => !cancelled && ingest(e));
        if (!cancelled) setMode({ kind: 'dashboard' });
      }
    };

    boot().catch(err => !cancelled && flash(err instanceof Error ? err.message : String(err)));

    const offChanged = client.config.onChanged(() => setReloadKey(k => k + 1));
    return () => {
      cancelled = true;
      offChanged();
      for (const t of timers.values()) clearTimeout(t);
      timers.clear();
      connRef.current?.close();
      connRef.current = null;
    };
  }, [client, ingest, flash, reloadKey]);

  const savePicker = useCallback(
    (ids: string[]) => {
      const conn = connRef.current;
      if (!conn) return;
      setSelection(ids);
      setEntities({});
      setMode({ kind: 'dashboard' });
      void (async () => {
        await client.store.put({ key: SELECTION_KEY, value: ids.join(',') });
        await conn.subscribeEntities(ids, ingest);
      })().catch(e => flash(errText(e)));
    },
    [client, ingest, flash],
  );

  const openPicker = useCallback(() => {
    const conn = connRef.current;
    if (!conn) return;
    conn
      .getStates()
      .then(all => setMode({ kind: 'picker', all }))
      .catch(e => flash(errText(e)));
  }, [flash]);

  const handleActivate = useCallback(
    (s: HaState) => {
      const conn = connRef.current;
      if (!conn) return;
      const kind = controlKind(s.entityId);
      if (kind === 'momentary') {
        const { domain, service } = momentaryCall(s.entityId);
        conn.callService(domain, service, {}, { entity_id: s.entityId }).catch(e => flash(errText(e)));
        return;
      }
      if (kind === 'toggle' || kind === 'lock') {
        const next = optimisticToggle(s);
        setOverlay(prev => ({ ...prev, [s.entityId]: next }));
        const { domain, service } = toggleCall(s);
        conn.callService(domain, service, {}, { entity_id: s.entityId }).catch(e => {
          setOverlay(prev => dropKey(prev, s.entityId));
          flash(errText(e));
        });
      }
    },
    [flash],
  );

  const handleSetTemp = useCallback(
    (entityId: string, target: number) => {
      const conn = connRef.current;
      if (!conn) return;
      setPendingTemp(prev => ({ ...prev, [entityId]: target }));
      const existing = tempTimers.current.get(entityId);
      if (existing) clearTimeout(existing);
      tempTimers.current.set(
        entityId,
        setTimeout(() => {
          tempTimers.current.delete(entityId);
          conn.callService('climate', 'set_temperature', { temperature: target }, { entity_id: entityId }).catch(e => {
            setPendingTemp(prev => dropKey(prev, entityId));
            flash(errText(e));
          });
        }, TEMP_DEBOUNCE_MS),
      );
    },
    [flash],
  );

  if (mode.kind === 'loading')
    return <Center muted={status.kind === 'error'}>{status.kind === 'error' ? status.message : 'connecting to home assistant...'}</Center>;
  if (mode.kind === 'needs-config') return <Center muted>{mode.message}</Center>;
  if (mode.kind === 'picker')
    return <Picker all={mode.all} initial={selection} onDone={savePicker} onCancel={selection.length ? () => setMode({ kind: 'dashboard' }) : undefined} />;

  const tiles = selection.map(id => mergeState(id, entities[id], overlay[id], pendingTemp[id]));
  return (
    <Dashboard
      tiles={tiles}
      status={status}
      toast={toast}
      onActivate={handleActivate}
      onSetTemp={handleSetTemp}
      onOpenPicker={openPicker}
    />
  );
}

export type Tile = { entityId: string; state: HaState | null; pendingTemp: number | null };

function mergeState(id: string, live: HaState | undefined, overlayState: string | undefined, pending: number | undefined): Tile {
  if (!live) return { entityId: id, state: null, pendingTemp: pending ?? null };
  const state = overlayState ? { ...live, state: overlayState } : live;
  return { entityId: id, state, pendingTemp: pending ?? null };
}

function Center({ children, muted }: { children: React.ReactNode; muted?: boolean }) {
  return (
    <div className="flex h-full w-full items-center justify-center bg-bt-charcoal px-10">
      <div className={`max-w-[34rem] text-center text-sm ${muted ? 'text-bt-soft-gray' : 'text-bt-off-white'}`}>{children}</div>
    </div>
  );
}

function splitIds(value: string): string[] {
  return value
    .split(',')
    .map(s => s.trim())
    .filter(Boolean);
}

function dropKey<T>(obj: Record<string, T>, key: string): Record<string, T> {
  if (!(key in obj)) return obj;
  const out = { ...obj };
  delete out[key];
  return out;
}

function numAttr(attr: unknown): number | null {
  return typeof attr === 'number' && Number.isFinite(attr) ? attr : null;
}

function errText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}
