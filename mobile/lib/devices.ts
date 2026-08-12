import type { KnownDevice } from './session';
import { relativeTime } from './utils';

export function linkSummary(device: KnownDevice, now = Date.now()): string {
  switch (device.peer?.status) {
    case 'connected':
      return 'connected';
    case 'linkFailed':
      return 'attached, but the link did not open';
    default:
      return device.lastConnectedAt > 0
        ? `last connected ${relativeTime(device.lastConnectedAt, now)}`
        : 'not connected';
  }
}
