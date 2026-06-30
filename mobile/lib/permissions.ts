import { Linking, Platform } from 'react-native';
import { check, PERMISSIONS, request, RESULTS } from 'react-native-permissions';

export type PermissionState = 'granted' | 'denied' | 'blocked' | 'unavailable';

const LOCATION =
  Platform.OS === 'android'
    ? PERMISSIONS.ANDROID.ACCESS_FINE_LOCATION
    : PERMISSIONS.IOS.LOCATION_WHEN_IN_USE;

// CDM does the scanning for us, so we never need BLUETOOTH_SCAN or location to
// find the Car Thing. But opening the RFCOMM socket - and starting the
// connectedDevice foreground service that hosts it - still requires
// BLUETOOTH_CONNECT to be granted at runtime on Android 12+.
const BLUETOOTH_CONNECT =
  Platform.OS === 'android' ? PERMISSIONS.ANDROID.BLUETOOTH_CONNECT : null;

function toState(result: string): PermissionState {
  if (result === RESULTS.GRANTED || result === RESULTS.LIMITED)
    return 'granted';
  if (result === RESULTS.BLOCKED) return 'blocked';
  if (result === RESULTS.UNAVAILABLE) return 'unavailable';
  return 'denied';
}

export async function locationStatus(): Promise<PermissionState> {
  return toState(await check(LOCATION));
}

export async function requestLocation(): Promise<PermissionState> {
  return toState(await request(LOCATION));
}

export async function bluetoothConnectStatus(): Promise<PermissionState> {
  if (!BLUETOOTH_CONNECT) return 'granted';
  return toState(await check(BLUETOOTH_CONNECT));
}

export async function requestBluetoothConnect(): Promise<PermissionState> {
  if (!BLUETOOTH_CONNECT) return 'granted';
  return toState(await request(BLUETOOTH_CONNECT));
}

export function openAppSettings(): void {
  Linking.openSettings().catch(() => {});
}
