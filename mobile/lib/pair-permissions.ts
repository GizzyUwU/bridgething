import { Platform } from 'react-native';
import {
  PERMISSIONS,
  RESULTS,
  openSettings,
  requestMultiple,
  type PermissionStatus,
} from 'react-native-permissions';

/** iOS doesn't gate BT behind runtime perms (EAAccessory). */
export type PairPermissionResult = 'granted' | 'denied' | 'blocked' | 'unavailable';

export async function requestPairPermissions(): Promise<PairPermissionResult> {
  if (Platform.OS !== 'android') return 'granted';
  const perms = [
    PERMISSIONS.ANDROID.BLUETOOTH_CONNECT,
    PERMISSIONS.ANDROID.BLUETOOTH_SCAN,
  ];
  let results: Record<string, PermissionStatus>;
  try {
    results = await requestMultiple(perms);
  } catch {
    return 'unavailable';
  }
  const statuses = Object.values(results);
  if (statuses.every(s => s === RESULTS.GRANTED)) return 'granted';
  if (statuses.some(s => s === RESULTS.BLOCKED)) return 'blocked';
  return 'denied';
}

export async function openSystemBluetoothSettings(): Promise<void> {
  await (Platform.OS === 'android' ? openSettings('bluetooth') : openSettings());
}
