import { useCallback, useSyncExternalStore } from 'react';
import { AppState, type AppStateStatus } from 'react-native';

import { isAppActive } from './app-active';
import {
  backgroundLocationStatus,
  locationStatus,
  type PermissionState,
} from './permissions';
import { getSession } from './session';

export type PermissionKey =
  | 'location'
  | 'backgroundLocation'
  | 'notificationAccess'
  | 'defaultDialer'
  | 'batteryExemption';

export type PermissionEntry = { state: PermissionState | null; busy: boolean };

export type PermissionMap = Record<PermissionKey, PermissionEntry>;

export type PermissionEvent =
  | { kind: 'reading'; keys: PermissionKey[] }
  | { kind: 'settled'; key: PermissionKey; state: PermissionState }
  | { kind: 'failed'; key: PermissionKey };

export const PERMISSION_KEYS: PermissionKey[] = [
  'location',
  'backgroundLocation',
  'notificationAccess',
  'defaultDialer',
  'batteryExemption',
];

export const INITIAL_PERMISSIONS: PermissionMap = Object.freeze(
  Object.fromEntries(
    PERMISSION_KEYS.map(key => [key, { state: null, busy: false }]),
  ),
) as PermissionMap;

export function reducePermissions(
  map: PermissionMap,
  event: PermissionEvent,
): PermissionMap {
  switch (event.kind) {
    case 'reading': {
      const touched = event.keys.filter(key => !map[key].busy);
      if (touched.length === 0) return map;
      const next = { ...map };
      for (const key of touched) next[key] = { ...map[key], busy: true };
      return next;
    }
    case 'settled': {
      const held = map[event.key];
      if (held.state === event.state && !held.busy) return map;
      return { ...map, [event.key]: { state: event.state, busy: false } };
    }
    case 'failed': {
      const held = map[event.key];
      if (!held.busy) return map;
      return { ...map, [event.key]: { ...held, busy: false } };
    }
  }
}

async function fromGrant(read: Promise<boolean>): Promise<PermissionState> {
  return (await read) ? 'granted' : 'denied';
}

const READERS: Record<PermissionKey, () => Promise<PermissionState>> = {
  location: locationStatus,
  backgroundLocation: backgroundLocationStatus,
  notificationAccess: () =>
    fromGrant(getSession().isNotificationAccessGranted()),
  defaultDialer: () => fromGrant(getSession().isDefaultDialer()),
  batteryExemption: () =>
    fromGrant(getSession().isIgnoringBatteryOptimizations()),
};

let map = INITIAL_PERMISSIONS;
let foreground = isAppActive(AppState.currentState);
let subscription: ReturnType<typeof AppState.addEventListener> | null = null;

const listeners = new Set<() => void>();
const watched = new Map<PermissionKey, number>();

function dispatch(event: PermissionEvent): void {
  const next = reducePermissions(map, event);
  if (next === map) return;
  map = next;
  for (const listener of listeners) listener();
}

async function read(keys: PermissionKey[]): Promise<void> {
  if (keys.length === 0) return;
  dispatch({ kind: 'reading', keys });
  await Promise.all(
    keys.map(async key => {
      try {
        dispatch({ kind: 'settled', key, state: await READERS[key]() });
      } catch {
        dispatch({ kind: 'failed', key });
      }
    }),
  );
}

function onAppState(next: AppStateStatus): void {
  const active = isAppActive(next);
  if (active === foreground) return;
  foreground = active;
  if (active) void read([...watched.keys()]);
}

function watch(key: PermissionKey, listener: () => void): () => void {
  listeners.add(listener);
  watched.set(key, (watched.get(key) ?? 0) + 1);
  if (!subscription)
    subscription = AppState.addEventListener('change', onAppState);
  if (!map[key].busy) void read([key]);

  return () => {
    listeners.delete(listener);
    const held = (watched.get(key) ?? 1) - 1;
    if (held > 0) watched.set(key, held);
    else watched.delete(key);
    if (watched.size === 0) {
      subscription?.remove();
      subscription = null;
    }
  };
}

export type PermissionStatus = PermissionEntry & {
  ready: boolean;
  granted: boolean;
  blocked: boolean;
  unavailable: boolean;
  run: (action: () => Promise<unknown>) => Promise<void>;
};

export function usePermissionStatus(key: PermissionKey): PermissionStatus {
  const entry = useSyncExternalStore(
    useCallback(listener => watch(key, listener), [key]),
    useCallback(() => map[key], [key]),
  );

  const run = useCallback(
    async (action: () => Promise<unknown>) => {
      dispatch({ kind: 'reading', keys: [key] });
      try {
        await action();
      } finally {
        await read([key]);
      }
    },
    [key],
  );

  return {
    ...entry,
    ready: entry.state !== null,
    granted: entry.state === 'granted',
    blocked: entry.state === 'blocked',
    unavailable: entry.state === 'unavailable',
    run,
  };
}
