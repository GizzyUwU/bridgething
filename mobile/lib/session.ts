import {
  BridgethingSession,
  type BridgethingAncsAuthStatus,
  type BridgethingAuthState,
  type BridgethingDeviceMeta,
  type BridgethingHostInfo,
  type BridgethingNowPlaying,
  type BridgethingProviderInfo,
  type BridgethingServiceHealth,
  type BridgethingSessionPeer,
  type BridgethingSessionSnapshot,
  type SessionEvent,
} from '@bridgething/session-react-native';
import { create } from 'zustand';
import { useShallow } from 'zustand/react/shallow';

import { startDiagnostics } from './diagnostics';
import { startOta } from './ota';
import {
  DEFAULT_CAPABILITY_FLAGS,
  DEFAULT_OTA_POLL_CONFIG,
  type DeviceLedgerEntry,
  forgetDevice as persistForget,
  getLedger,
  recordDeviceSeen,
  setDeviceNickname,
} from './storage';

let sessionSingleton: BridgethingSession | null = null;

export function getSession(): BridgethingSession {
  if (!sessionSingleton) sessionSingleton = new BridgethingSession();
  return sessionSingleton;
}

type SessionState = {
  started: boolean;

  provider: BridgethingProviderInfo | null;
  authState: BridgethingAuthState;
  serviceHealth: BridgethingServiceHealth;
  peers: BridgethingSessionPeer[];
  ancsAuthStatus: BridgethingAncsAuthStatus;
  nowPlaying: BridgethingNowPlaying | null;
  deviceMeta: Record<string, BridgethingDeviceMeta>;
  hostInfo: BridgethingHostInfo | null;
  ledger: Record<string, DeviceLedgerEntry>;
  capabilityFlags: typeof DEFAULT_CAPABILITY_FLAGS;
  otaPollConfig: typeof DEFAULT_OTA_POLL_CONFIG | null;

  apply(event: SessionEvent): void;
  reconcile(snapshot: BridgethingSessionSnapshot): void;
  setAuthState(state: BridgethingAuthState): void;
  reset(): void;
};

const initial: Omit<
  SessionState,
  'apply' | 'reconcile' | 'reset' | 'setAuthState'
> = {
  started: false,
  provider: null,
  authState: { kind: 'idle' },
  serviceHealth: { kind: 'ok' },
  peers: [],
  ancsAuthStatus: 'unknown',
  nowPlaying: null,
  deviceMeta: {},
  hostInfo: null,
  ledger: getLedger(),
  capabilityFlags: { ...DEFAULT_CAPABILITY_FLAGS },
  otaPollConfig: null,
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
      case 'serviceHealthChanged':
        set({ serviceHealth: event.health });
        return;
      case 'peerConnected':
        set(s => {
          const others = s.peers.filter(p => p.id !== event.peer.id);
          const ledger = recordDeviceSeen(
            event.peer.id,
            event.peer.name,
            Date.now(),
          );
          return { peers: [...others, event.peer], ledger };
        });
        return;
      case 'peerLinkFailed':
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
      case 'diagEntry':
        return;
    }
  },
  reconcile: snapshot => {
    const now = Date.now();
    let ledger = getLedger();
    for (const peer of snapshot.peers) {
      if (peer.status === 'connected') {
        ledger = recordDeviceSeen(peer.id, peer.name, now);
      }
    }
    set({
      provider: snapshot.provider ?? null,
      authState: snapshot.authState,
      serviceHealth: snapshot.serviceHealth,
      peers: snapshot.peers,
      ancsAuthStatus: snapshot.ancsAuthStatus,
      nowPlaying: snapshot.nowPlaying ?? null,
      deviceMeta: Object.fromEntries(
        snapshot.deviceMeta.map(e => [e.deviceId, e.meta]),
      ),
      hostInfo: snapshot.hostInfo,
      capabilityFlags: snapshot.capabilityFlags,
      otaPollConfig: snapshot.otaPollConfig ?? null,
      ledger,
    });
  },
  setAuthState: state => set({ authState: state }),
  reset: () => set({ ...initial }),
}));

let wired = false;

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
  await reconcileSnapshot();
  await startDiagnostics();
  startOta();
}

export async function reconcileSnapshot(): Promise<void> {
  try {
    const snapshot = await getSession().snapshot();
    useSessionStore.getState().reconcile(snapshot);
  } catch (err) {
    console.warn('[bridgething] snapshot reconcile failed', err);
  }
}

export async function updateCapabilityFlags(
  flags: typeof DEFAULT_CAPABILITY_FLAGS,
): Promise<void> {
  useSessionStore.setState({ capabilityFlags: flags });
  await getSession().setCapabilityFlags(flags);
}

export async function updateOtaPollConfig(
  config: typeof DEFAULT_OTA_POLL_CONFIG | null,
): Promise<void> {
  useSessionStore.setState({ otaPollConfig: config });
  await getSession().setOtaPollConfig(config);
}

export function updateNickname(
  deviceId: string,
  nickname: string | null,
): void {
  useSessionStore.setState({ ledger: setDeviceNickname(deviceId, nickname) });
}

export function forgetKnownDevice(deviceId: string): void {
  useSessionStore.setState({ ledger: persistForget(deviceId) });
}

export function waitForPeer(timeoutMs: number): Promise<boolean> {
  const connected = useSessionStore
    .getState()
    .peers.some(p => p.status === 'connected');
  if (connected) return Promise.resolve(true);
  return new Promise(resolve => {
    let unsub: (() => void) | null = null;
    const done = (ok: boolean) => {
      unsub?.();
      unsub = null;
      clearTimeout(timer);
      resolve(ok);
    };
    const timer = setTimeout(() => done(false), timeoutMs);
    unsub = getSession().subscribe(event => {
      if (event.type === 'peerConnected') done(true);
    });
  });
}

export function connectedPeers(
  peers: BridgethingSessionPeer[],
): BridgethingSessionPeer[] {
  return peers.filter(p => p.status === 'connected');
}

export function peerDisplayName(
  peer: BridgethingSessionPeer,
  ledger: Record<string, DeviceLedgerEntry>,
): string {
  return ledger[peer.id]?.nickname ?? peer.name;
}

export type KnownDevice = {
  id: string;
  displayName: string;
  nickname: string | null;
  lastConnectedAt: number;
  peer: BridgethingSessionPeer | null;
};

export function knownDevices(
  ledger: Record<string, DeviceLedgerEntry>,
  peers: BridgethingSessionPeer[],
): KnownDevice[] {
  const byId = new Map<string, KnownDevice>();
  for (const entry of Object.values(ledger)) {
    byId.set(entry.id, {
      id: entry.id,
      displayName: entry.nickname ?? entry.lastName,
      nickname: entry.nickname,
      lastConnectedAt: entry.lastConnectedAt,
      peer: null,
    });
  }
  for (const peer of peers) {
    const prior = byId.get(peer.id);
    byId.set(peer.id, {
      id: peer.id,
      displayName: prior?.nickname ?? peer.name,
      nickname: prior?.nickname ?? null,
      lastConnectedAt: prior?.lastConnectedAt ?? 0,
      peer,
    });
  }
  return [...byId.values()].sort((a, b) => {
    const aConn = a.peer?.status === 'connected' ? 1 : 0;
    const bConn = b.peer?.status === 'connected' ? 1 : 0;
    if (aConn !== bConn) return bConn - aConn;
    return b.lastConnectedAt - a.lastConnectedAt;
  });
}

export function useSession<T>(selector: (state: SessionState) => T): T {
  return useSessionStore(useShallow(selector));
}

function omit<T extends object>(obj: T, key: keyof T | string): T {
  const next = { ...obj } as Record<string, unknown>;
  delete next[key as string];
  return next as T;
}
