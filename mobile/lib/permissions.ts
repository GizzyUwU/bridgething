import { Linking, Platform } from 'react-native';
import { check, PERMISSIONS, request, RESULTS } from 'react-native-permissions';

export type PermissionState = 'granted' | 'denied' | 'blocked' | 'unavailable';

const LOCATION =
  Platform.OS === 'android'
    ? PERMISSIONS.ANDROID.ACCESS_FINE_LOCATION
    : PERMISSIONS.IOS.LOCATION_WHEN_IN_USE;

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

export function openAppSettings(): void {
  Linking.openSettings().catch(() => {});
}
