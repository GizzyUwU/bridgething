import {
  BridgethingSession,
  type BridgethingAncsAuthStatus,
  type BridgethingAuthState,
  type BridgethingDeviceMeta,
  type BridgethingNowPlaying,
  type BridgethingProviderInfo,
  type BridgethingSessionPeer,
  type SessionEvent,
} from '@bridgething/session-react-native';
import { useEffect } from 'react';
import { create } from 'zustand';
import { useShallow } from 'zustand/react/shallow';

import {
  DEFAULT_CAPABILITY_FLAGS,
  DEFAULT_OTA_POLL_CONFIG,
  getAllNicknames,
  getCapabilityFlags,
  getOtaPollConfig,
  setCapabilityFlags as persistCapabilityFlags,
  setNickname as persistNickname,
  setOtaPollConfig as persistOtaPollConfig,
} from './storage';

let sessionSingleton: BridgethingSession | null = null;

/** Process-wide session instance. Lazy so RN hot reloads don't double-
 *  register Nitro callbacks. */
export function getSession(): BridgethingSession {
  if (!sessionSingleton) sessionSingleton = new BridgethingSession();
  return sessionSingleton;
}

type SessionState = {
  /** Lazily mutated by `bootstrapSession`. UI gates on this to
   *  decide initial route and whether settings/dashboard can talk
   *  to the companion yet. */
  started: boolean;

  provider: BridgethingProviderInfo | null;
  authState: BridgethingAuthState;
  peers: BridgethingSessionPeer[];
  ancsAuthStatus: BridgethingAncsAuthStatus;
  nowPlaying: BridgethingNowPlaying | null;
  deviceMeta: Record<string, BridgethingDeviceMeta>;
  /** User-assigned nicknames keyed by deviceId. Mirrors mmkv. */
  nicknames: Record<string, string>;
  /** Live snapshot of capability flags as JS sees them. Mirrors mmkv. */
  capabilityFlags: typeof DEFAULT_CAPABILITY_FLAGS;
  /** Null = polling disabled. Mirrors mmkv. */
  otaPollConfig: typeof DEFAULT_OTA_POLL_CONFIG | null;

  apply(event: SessionEvent): void;
  reset(): void;
};

const initial: Omit<SessionState, 'apply' | 'reset'> = {
  started: false,
  provider: null,
  authState: { kind: 'idle' },
  peers: [],
  ancsAuthStatus: 'unknown',
  nowPlaying: null,
  deviceMeta: {},
  nicknames: getAllNicknames(),
  capabilityFlags: getCapabilityFlags(),
  otaPollConfig: getOtaPollConfig(),
};

export const useSessionStore = create<SessionState>((set, _get) => ({
  ...initial,
  apply: event => {
    switch (event.type) {
      case 'providerChanged':
        set({ provider: event.provider });
        return;
      case 'authStateChanged':
        set({ authState: event.state });
        return;
      case 'peerConnected':
        set(s => {
          const others = s.peers.filter(p => p.id !== event.peer.id);
          return { peers: [...others, event.peer] };
        });
        return;
      case 'peerDisconnected':
        set(s => ({
          peers: s.peers.filter(p => p.id !== event.peerId),
          deviceMeta: omit(s.deviceMeta, event.peerId),
        }));
        return;
      case 'ancsAuthStatusChanged':
        set({ ancsAuthStatus: event.status });
        return;
      case 'nowPlayingChanged':
        set({ nowPlaying: event.nowPlaying });
        return;
      case 'deviceMetaChanged':
        set(s => ({
          deviceMeta: { ...s.deviceMeta, [event.deviceId]: event.meta },
        }));
        return;
      case 'webappsChanged':
      case 'otaEvent':
      case 'log':
        // Webapps changes are observed by the dashboard via dedicated
        // selectors / subscriptions; OTA events + logs are streamed to
        // ad-hoc consumers in their own components.
        return;
    }
  },
  reset: () => set({ ...initial }),
}));

let wired = false;

/**
 * Subscribe the zustand store to the native event stream once and start
 * the session. Idempotent across hot reloads. Called from App.tsx.
 *
 * Also pushes mmkv-resident config (capability flags, OTA poll config)
 * down to native after start, so the companion ends up with the user's
 * choices instead of the all-off defaults the Swift impl boots with.
 */
export async function bootstrapSession(): Promise<void> {
  const session = getSession();
  const store = useSessionStore.getState();

  if (!wired) {
    session.subscribe(event => store.apply(event));
    wired = true;
  }

  if (store.started) return;
  await session.start();
  useSessionStore.setState({ started: true });

  // Apply persisted preferences. Order: flags (cheap; pure config),
  // poll config (may schedule a network call). Errors here are
  // non-fatal — the user can fix them in Settings.
  try {
    await session.setCapabilityFlags(store.capabilityFlags);
  } catch (err) {
    console.warn('[bridgething] setCapabilityFlags on bootstrap failed', err);
  }
  try {
    await session.setOtaPollConfig(store.otaPollConfig);
  } catch (err) {
    console.warn('[bridgething] setOtaPollConfig on bootstrap failed', err);
  }
}

// MARK: - Convenience selectors / mutators

/** Push a flag change to native and mmkv at the same time. The optimistic
 *  store update lands first so toggles feel instant. */
export async function updateCapabilityFlags(
  flags: typeof DEFAULT_CAPABILITY_FLAGS,
): Promise<void> {
  useSessionStore.setState({ capabilityFlags: flags });
  persistCapabilityFlags(flags);
  await getSession().setCapabilityFlags(flags);
}

export async function updateOtaPollConfig(
  config: typeof DEFAULT_OTA_POLL_CONFIG | null,
): Promise<void> {
  useSessionStore.setState({ otaPollConfig: config });
  persistOtaPollConfig(config);
  await getSession().setOtaPollConfig(config);
}

/** JS-only nickname update. Mirrors to mmkv. The store layer reads
 *  nicknames by id when rendering peer rows. */
export function updateNickname(
  deviceId: string,
  nickname: string | null,
): void {
  useSessionStore.setState(s => {
    const next = { ...s.nicknames };
    const trimmed = nickname?.trim();
    if (trimmed && trimmed.length > 0) next[deviceId] = trimmed;
    else delete next[deviceId];
    return { nicknames: next };
  });
  persistNickname(deviceId, nickname);
}

/** Pretty name for a peer — nickname when set, BT-advertised name otherwise. */
export function peerDisplayName(
  peer: BridgethingSessionPeer,
  nicknames: Record<string, string>,
): string {
  return nicknames[peer.id] ?? peer.name;
}

// MARK: - Hook helpers

/**
 * Subscribe to a slice of session state. `selector` is wrapped in
 * `useShallow` so consumers can return arrays / objects without the
 * fresh-allocation infinite-loop trap.
 */
export function useSession<T>(selector: (state: SessionState) => T): T {
  return useSessionStore(useShallow(selector));
}

/**
 * Push the native log stream on while the calling screen is mounted.
 * Refcounted across multiple Logs screens (multi-window etc.).
 */
export function useLogStream(
  handler: (level: string, message: string) => void,
): void {
  const session = getSession();
  useEffect(() => {
    session.setLogStreamingEnabled(true);
    const off = session.subscribe(event => {
      if (event.type === 'log') handler(event.level, event.message);
    });
    return () => {
      off();
      session.setLogStreamingEnabled(false);
    };
    // handler is captured by the subscriber; consumers should pass a
    // stable callback (useCallback) when the inner state changes.
  }, [handler, session]);
}

function omit<T extends object>(obj: T, key: keyof T | string): T {
  const next = { ...obj } as Record<string, unknown>;
  delete next[key as string];
  return next as T;
}
