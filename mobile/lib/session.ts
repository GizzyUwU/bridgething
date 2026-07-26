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
import { Alert, AppState, Platform } from 'react-native';
import { create } from 'zustand';
import { useShallow } from 'zustand/react/shallow';

import { startDiagnostics } from './diagnostics';
import { startOta } from './ota';
import { requestBluetoothConnect } from './permissions';
import { refreshWebapps, startWebapps } from './webapps';
import {
  DEFAULT_CAPABILITY_FLAGS,
  DEFAULT_OTA_POLL_CONFIG,
  type DeviceLedgerEntry,
  forgetDevice as persistForget,
  getLedger,
  recordDeviceSeen,
  recordDeviceSerial,
  setDeviceNickname,
} from './storage';

let sessionSingleton: BridgethingSession | null = null;

export function getSession(): BridgethingSession {
  if (!sessionSingleton) sessionSingleton = new BridgethingSession();
  return sessionSingleton;
}

type SessionState = {
  started: boolean;

  providers: BridgethingProviderInfo[];
  providerPriority: string[];
  libraryProvider: string | null;
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
  reset(): void;
};

const initial: Omit<SessionState, 'apply' | 'reconcile' | 'reset'> = {
  started: false,
  providers: [],
  providerPriority: [],
  libraryProvider: null,
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
      case 'providersChanged':
        set({ providers: event.providers });
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
          ledger: event.meta.serialNumber
            ? recordDeviceSerial(event.deviceId, event.meta.serialNumber)
            : s.ledger,
        }));
        return;
      case 'webappsChanged':
      case 'otaEvent':
      case 'log':
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
    for (const entry of snapshot.deviceMeta) {
      if (entry.meta.serialNumber) {
        ledger = recordDeviceSerial(entry.deviceId, entry.meta.serialNumber);
      }
    }
    set({
      providers: snapshot.providers,
      providerPriority: snapshot.providerPriority,
      libraryProvider: snapshot.libraryProvider ?? null,
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
  reset: () => set({ ...initial }),
}));

let wired = false;

export async function bootstrapSession(): Promise<void> {
  const session = getSession();
  const store = useSessionStore.getState();

  if (!wired) {
    session.subscribe(event => store.apply(event));
    AppState.addEventListener('change', state => {
      if (state === 'active') void reconcileSnapshot();
    });
    wired = true;
  }

  if (store.started) return;
  await session.start();
  useSessionStore.setState({ started: true });
  await reconcileSnapshot();
  await startDiagnostics();
  startOta();
  startWebapps();
  for (const peer of useSessionStore.getState().peers)
    if (peer.status === 'connected') void refreshWebapps(peer.id);
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

export async function setDeviceName(
  deviceId: string,
  name: string | null,
): Promise<void> {
  await getSession().deviceSetNickname(deviceId, name ?? '');
}

export function forgetKnownDevice(deviceId: string): void {
  void getSession()
    .forgetCompanionDevice(deviceId)
    .catch(() => {});
  useSessionStore.setState({ ledger: persistForget(deviceId) });
}

export async function presentPairWithGuidance(): Promise<boolean> {
  const picked = await getSession().presentPairPicker();
  if (picked == null && Platform.OS === 'ios') {
    Alert.alert(
      'pairing did not finish',
      `if your Car Thing was paired to this phone before, forget it first: open Settings > Bluetooth, tap your Car Thing, choose "Forget This Device", then pair again.`,
    );
  }
  return picked != null;
}

export type PairOutcome =
  | { kind: 'connected' }
  | { kind: 'cancelled' }
  | { kind: 'permissionDenied' }
  | { kind: 'pairingFailed' }
  | { kind: 'timeout' }
  | { kind: 'notificationsFailed'; message?: string }
  | { kind: 'error'; message: string };

export async function runPairFlow(): Promise<PairOutcome> {
  try {
    if (Platform.OS === 'android') {
      const bt = await requestBluetoothConnect();
      if (bt !== 'granted') return { kind: 'permissionDenied' };
      const picked = await getSession().presentPairPicker();
      if (picked == null) return { kind: 'cancelled' };
      if (picked.bondState !== 'bonded') return { kind: 'pairingFailed' };
      return (await waitForPeer(45000))
        ? { kind: 'connected' }
        : { kind: 'timeout' };
    }
    if (!(await presentPairWithGuidance())) return { kind: 'cancelled' };
    if (!(await waitForPeer(20000))) return { kind: 'timeout' };
    const ancs = await getSession().enableAncsNotifications();
    if (ancs.kind === 'failed') {
      return {
        kind: 'notificationsFailed',
        message: ancs.message ?? undefined,
      };
    }
    return { kind: 'connected' };
  } catch (err) {
    return {
      kind: 'error',
      message: err instanceof Error ? err.message : String(err),
    };
  }
}

export function alertPairOutcome(outcome: PairOutcome): void {
  switch (outcome.kind) {
    case 'permissionDenied':
      Alert.alert(
        'bluetooth permission needed',
        'bridgething needs Bluetooth access to connect to your Car Thing. enable it in settings, then try pairing again.',
      );
      return;
    case 'pairingFailed':
      Alert.alert(
        'pairing failed',
        'your Car Thing did not finish pairing. make sure it is powered on and nearby, then try again.',
      );
      return;
    case 'timeout':
      Alert.alert(
        Platform.OS === 'android' ? 'still connecting' : 'could not connect',
        Platform.OS === 'android'
          ? 'your Car Thing paired but has not connected yet. make sure it is on and nearby - it can take a few seconds.'
          : 'pairing finished but your Car Thing did not connect. make sure it is powered on and nearby, then try again.',
      );
      return;
    case 'notificationsFailed':
      Alert.alert(
        'notifications setup failed',
        outcome.message ??
          'pairing worked, but enabling notifications did not.',
      );
      return;
    case 'error':
      Alert.alert('pairing failed', outcome.message);
      return;
    case 'connected':
    case 'cancelled':
      return;
  }
}

export function waitForPeer(timeoutMs: number): Promise<boolean> {
  const isConnected = () =>
    useSessionStore.getState().peers.some(p => p.status === 'connected');
  if (isConnected()) return Promise.resolve(true);
  return new Promise(resolve => {
    let unsub: (() => void) | null = null;
    const done = (ok: boolean) => {
      unsub?.();
      unsub = null;
      clearTimeout(timer);
      resolve(ok);
    };
    const timer = setTimeout(() => done(false), timeoutMs);
    unsub = useSessionStore.subscribe(state => {
      if (state.peers.some(p => p.status === 'connected')) done(true);
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
  meta?: BridgethingDeviceMeta,
): string {
  return ledger[peer.id]?.nickname ?? meta?.nickname ?? peer.name;
}

export type KnownDevice = {
  id: string;
  displayName: string;
  nickname: string | null;
  lastConnectedAt: number;
  serialNumber: string | null;
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
      serialNumber: entry.serialNumber,
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
      serialNumber: prior?.serialNumber ?? null,
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
