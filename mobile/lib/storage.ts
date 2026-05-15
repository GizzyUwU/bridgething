import { createMMKV } from 'react-native-mmkv';

import type {
  BridgethingCapabilityFlags,
  BridgethingOtaPollConfig,
} from '@bridgething/session-react-native';

/** Single mmkv instance for everything the JS side persists. mmkv is
 *  fast enough that we don't need to lazy-instantiate per domain;
 *  prefix keys instead. */
export const storage = createMMKV({ id: 'bridgething' });

const KEY = {
  setupCompleted: 'setup.completed',
  nicknames: 'device.nicknames', // JSON { [deviceId]: nickname }
  capabilityFlags: 'flags.capabilities', // JSON of CapabilityFlags
  otaPollConfig: 'ota.pollConfig', // JSON of OtaPollConfig | null marker
} as const;

/** Default capability-flag profile applied on first launch and used as
 *  the starting point for every settings toggle. Off by default — opt-in. */
export const DEFAULT_CAPABILITY_FLAGS: BridgethingCapabilityFlags = {
  geo: false,
  notifications: false,
  netFetch: false,
  netWs: false,
  audioTts: false,
};

/** Default OTA poll config when the user hasn't picked one yet. */
export const DEFAULT_OTA_POLL_CONFIG: BridgethingOtaPollConfig = {
  channel: 'stable',
  intervalSeconds: 21600,
  autoPush: true,
};

// First-run gate

export function getSetupCompleted(): boolean {
  return storage.getBoolean(KEY.setupCompleted) ?? false;
}

export function setSetupCompleted(value: boolean): void {
  storage.set(KEY.setupCompleted, value);
}

// Device nicknames

type NicknameMap = Record<string, string>;

function readNicknameMap(): NicknameMap {
  const raw = storage.getString(KEY.nicknames);
  if (!raw) return {};
  try {
    return JSON.parse(raw) as NicknameMap;
  } catch {
    return {};
  }
}

function writeNicknameMap(map: NicknameMap): void {
  storage.set(KEY.nicknames, JSON.stringify(map));
}

export function getNickname(deviceId: string): string | null {
  return readNicknameMap()[deviceId] ?? null;
}

export function setNickname(deviceId: string, nickname: string | null): void {
  const map = readNicknameMap();
  if (nickname && nickname.trim().length > 0) {
    map[deviceId] = nickname.trim();
  } else {
    delete map[deviceId];
  }
  writeNicknameMap(map);
}

export function getAllNicknames(): NicknameMap {
  return readNicknameMap();
}

// Capability flags

export function getCapabilityFlags(): BridgethingCapabilityFlags {
  const raw = storage.getString(KEY.capabilityFlags);
  if (!raw) return { ...DEFAULT_CAPABILITY_FLAGS };
  try {
    const stored = JSON.parse(raw) as Partial<BridgethingCapabilityFlags>;
    return { ...DEFAULT_CAPABILITY_FLAGS, ...stored };
  } catch {
    return { ...DEFAULT_CAPABILITY_FLAGS };
  }
}

export function setCapabilityFlags(flags: BridgethingCapabilityFlags): void {
  storage.set(KEY.capabilityFlags, JSON.stringify(flags));
}

// OTA poll config — null is a meaningful "user disabled polling" state.

export function getOtaPollConfig(): BridgethingOtaPollConfig | null {
  const raw = storage.getString(KEY.otaPollConfig);
  if (raw == null) return null;
  if (raw === 'null') return null;
  try {
    return JSON.parse(raw) as BridgethingOtaPollConfig;
  } catch {
    return null;
  }
}

export function setOtaPollConfig(
  config: BridgethingOtaPollConfig | null,
): void {
  storage.set(
    KEY.otaPollConfig,
    config == null ? 'null' : JSON.stringify(config),
  );
}
