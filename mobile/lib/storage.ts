import { createMMKV } from 'react-native-mmkv';

import type {
  BridgethingCapabilityFlags,
  BridgethingOtaPollConfig,
} from '@bridgething/session-react-native';

export const storage = createMMKV({ id: 'bridgething' });

const KEY = {
  setupCompleted: 'setup.completed',
  ledger: 'device.ledger', // JSON { [deviceId]: DeviceLedgerEntry }
} as const;

export const DEFAULT_CAPABILITY_FLAGS: BridgethingCapabilityFlags = {
  geo: true,
  notifications: true,
  netFetch: true,
  netWs: true,
  audioTts: true,
};

export const DEFAULT_OTA_POLL_CONFIG: BridgethingOtaPollConfig = {
  intervalSeconds: 3600,
  autoPush: true,
};

export function getSetupCompleted(): boolean {
  return storage.getBoolean(KEY.setupCompleted) ?? false;
}

export function setSetupCompleted(value: boolean): void {
  storage.set(KEY.setupCompleted, value);
}

export type DeviceLedgerEntry = {
  id: string;
  lastName: string;
  nickname: string | null;
  lastConnectedAt: number;
  serialNumber: string | null;
  libVersion: string | null;
};

type Ledger = Record<string, DeviceLedgerEntry>;

function readLedger(): Ledger {
  const raw = storage.getString(KEY.ledger);
  if (!raw) return {};
  try {
    return JSON.parse(raw) as Ledger;
  } catch {
    return {};
  }
}

function writeLedger(ledger: Ledger): void {
  storage.set(KEY.ledger, JSON.stringify(ledger));
}

export function getLedger(): Ledger {
  return readLedger();
}

export function recordDeviceSeen(
  id: string,
  name: string | null,
  atMs: number,
): Ledger {
  const ledger = readLedger();
  const prior = ledger[id];
  ledger[id] = {
    id,
    lastName: name ?? prior?.lastName ?? '',
    nickname: prior?.nickname ?? null,
    lastConnectedAt: atMs,
    serialNumber: prior?.serialNumber ?? null,
    libVersion: prior?.libVersion ?? null,
  };
  writeLedger(ledger);
  return ledger;
}

export type DeviceFacts = {
  serialNumber: string | null;
  nickname: string | null;
  libVersion: string | null;
};

export function recordDeviceMeta(id: string, facts: DeviceFacts): Ledger {
  const ledger = readLedger();
  const prior = ledger[id];
  const trimmed = facts.nickname?.trim();
  const next: DeviceFacts = {
    nickname: trimmed && trimmed.length > 0 ? trimmed : null,
    serialNumber: facts.serialNumber ?? prior?.serialNumber ?? null,
    libVersion: facts.libVersion ?? prior?.libVersion ?? null,
  };
  if (
    prior &&
    prior.serialNumber === next.serialNumber &&
    prior.nickname === next.nickname &&
    prior.libVersion === next.libVersion
  )
    return ledger;
  ledger[id] = {
    id,
    lastName: prior?.lastName ?? '',
    lastConnectedAt: prior?.lastConnectedAt ?? 0,
    ...next,
  };
  writeLedger(ledger);
  return ledger;
}

export function forgetDevice(id: string): Ledger {
  const ledger = readLedger();
  delete ledger[id];
  writeLedger(ledger);
  return ledger;
}
