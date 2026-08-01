import {
  type BridgethingAncsAuthStatus,
  type BridgethingDeviceMeta,
  type BridgethingHostInfo,
  type BridgethingNowPlaying,
  type BridgethingProviderInfo,
  type BridgethingSessionPeer,
} from '@bridgething/session-react-native';
import { Alert, Platform } from 'react-native';
import { create } from 'zustand';
import { useShallow } from 'zustand/react/shallow';

import {
  getSession,
  reconcileAll,
  registerDomain,
  startBridge,
} from './bridge';
import { startDiagnostics } from './diagnostics';
import { registerOtaDomain } from './ota';
import { requestBluetoothConnect } from './permissions';
import { registerWebappsDomain } from './webapps';
import {
  DEFAULT_CAPABILITY_FLAGS,
  DEFAULT_OTA_POLL_CONFIG,
  type DeviceLedgerEntry,
  forgetDevice as persistForget,
  getLedger,
  recordDeviceMeta,
  recordDeviceSeen,
} from './storage';

export { getSession } from './bridge';

export type SessionState = {
  started: boolean;
  reconciled: boolean;

  providers: BridgethingProviderInfo[];
  providerPriority: string[];
  libraryProvider: string | null;
  peers: BridgethingSessionPeer[];
  ancsAuthStatus: Record<string, BridgethingAncsAuthStatus>;
  nowPlaying: BridgethingNowPlaying | null;
  deviceMeta: Record<string, BridgethingDeviceMeta>;
  hostInfo: BridgethingHostInfo | null;
  ledger: Record<string, DeviceLedgerEntry>;
  capabilityFlags: typeof DEFAULT_CAPABILITY_FLAGS;
  otaPollConfig: typeof DEFAULT_OTA_POLL_CONFIG | null;
};

const initial: SessionState = {
  started: false,
  reconciled: false,
  providers: [],
  providerPriority: [],
  libraryProvider: null,
  peers: [],
  ancsAuthStatus: {},
  nowPlaying: null,
  deviceMeta: {},
  hostInfo: null,
  ledger: getLedger(),
  capabilityFlags: { ...DEFAULT_CAPABILITY_FLAGS },
  otaPollConfig: null,
};

export const useSessionStore = create<SessionState>(() => ({ ...initial }));

const set = useSessionStore.setState;

export function registerSessionDomain(): void {
  registerDomain({
    name: 'session',
    apply: event => {
      switch (event.type) {
        case 'providersChanged':
          set({ providers: event.providers });
          return;
        case 'peerConnected':
          set(s => ({
            peers: [...s.peers.filter(p => p.id !== event.peer.id), event.peer],
            ledger: recordDeviceSeen(
              event.peer.id,
              event.peer.name,
              Date.now(),
            ),
          }));
          void resyncOnReconnect();
          return;
        case 'peerLinkFailed':
          set(s => ({
            peers: [...s.peers.filter(p => p.id !== event.peer.id), event.peer],
          }));
          return;
        case 'peerDisconnected':
          set(s => ({
            peers: s.peers.filter(p => p.id !== event.peerId),
            deviceMeta: omit(s.deviceMeta, event.peerId),
            ledger: recordDeviceSeen(event.peerId, null, Date.now()),
          }));
          return;
        case 'ancsAuthStatusChanged':
          set(s => ({
            ancsAuthStatus: {
              ...s.ancsAuthStatus,
              [event.deviceId]: event.status,
            },
          }));
          return;
        case 'nowPlayingChanged':
          set({ nowPlaying: event.nowPlaying });
          return;
        case 'deviceMetaChanged':
          set(s => ({
            deviceMeta: { ...s.deviceMeta, [event.deviceId]: event.meta },
            ledger: recordDeviceMeta(event.deviceId, {
              serialNumber: event.meta.serialNumber ?? null,
              nickname: event.meta.nickname ?? null,
              libVersion: event.meta.libbridgethingVersion ?? null,
            }),
          }));
          return;
        case 'webappsChanged':
        case 'webappDocChanged':
        case 'otaRunChanged':
        case 'otaAvailableChanged':
        case 'otaPollChanged':
        case 'resumed':
        case 'log':
          return;
      }
    },
    reconcile: snapshot => {
      const now = Date.now();
      let ledger = getLedger();
      for (const peer of snapshot.peers) {
        if (peer.status === 'connected')
          ledger = recordDeviceSeen(peer.id, peer.name, now);
      }
      for (const entry of snapshot.deviceMeta) {
        ledger = recordDeviceMeta(entry.deviceId, {
          serialNumber: entry.meta.serialNumber ?? null,
          nickname: entry.meta.nickname ?? null,
          libVersion: entry.meta.libbridgethingVersion ?? null,
        });
      }
      set({
        providers: snapshot.providers,
        providerPriority: snapshot.providerPriority,
        libraryProvider: snapshot.libraryProvider ?? null,
        peers: snapshot.peers,
        ancsAuthStatus: Object.fromEntries(
          snapshot.ancsAuthStatuses.map(e => [e.deviceId, e.status]),
        ),
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
  });
}

let resyncInFlight: Promise<void> | null = null;

function resyncOnReconnect(): Promise<void> {
  resyncInFlight ??= reconcileAll()
    .catch((err: unknown) => {
      console.warn('[bridgething] reconcile on peer connect failed', err);
    })
    .finally(() => {
      resyncInFlight = null;
    });
  return resyncInFlight;
}

export async function bootstrapSession(): Promise<void> {
  registerSessionDomain();
  registerWebappsDomain();
  registerOtaDomain();
  startBridge();
  if (useSessionStore.getState().started) return;
  await getSession().start();
  useSessionStore.setState({ started: true });
  try {
    await reconcileAll();
  } catch (err) {
    console.warn('[bridgething] initial reconcile failed', err);
  }
  useSessionStore.setState({ reconciled: true });
  await startDiagnostics();
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
    const paired = useSessionStore
      .getState()
      .peers.find(p => p.status === 'connected');
    if (paired == null) return { kind: 'timeout' };
    const ancs = await getSession().enableAncsNotifications(paired.id);
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
): string {
  return ledger[peer.id]?.nickname ?? peer.name;
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
