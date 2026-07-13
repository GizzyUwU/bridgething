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
  name: string,
  atMs: number,
): Ledger {
  const ledger = readLedger();
  ledger[id] = {
    id,
    lastName: name,
    nickname: ledger[id]?.nickname ?? null,
    lastConnectedAt: atMs,
    serialNumber: ledger[id]?.serialNumber ?? null,
  };
  writeLedger(ledger);
  return ledger;
}

export function setDeviceNickname(id: string, nickname: string | null): Ledger {
  const ledger = readLedger();
  const prior = ledger[id];
  const trimmed = nickname?.trim();
  ledger[id] = {
    id,
    lastName: prior?.lastName ?? '',
    nickname: trimmed && trimmed.length > 0 ? trimmed : null,
    lastConnectedAt: prior?.lastConnectedAt ?? 0,
    serialNumber: prior?.serialNumber ?? null,
  };
  writeLedger(ledger);
  return ledger;
}

export function recordDeviceSerial(id: string, serialNumber: string): Ledger {
  const ledger = readLedger();
  const prior = ledger[id];
  if (prior?.serialNumber === serialNumber) return ledger;
  ledger[id] = {
    id,
    lastName: prior?.lastName ?? '',
    nickname: prior?.nickname ?? null,
    lastConnectedAt: prior?.lastConnectedAt ?? 0,
    serialNumber,
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
