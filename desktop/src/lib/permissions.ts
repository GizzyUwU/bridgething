import type { IconName } from './icons.tsx';

export type PermissionCopy = { icon: IconName; title: string; subtitle?: string };

export function humanizePermission(permission: string): PermissionCopy {
  switch (permission) {
    case 'net.fetch':
      return { icon: 'globe', title: 'use the internet', subtitle: 'requests are relayed through this computer' };
    case 'net.ws':
      return { icon: 'wifi', title: 'real-time data', subtitle: 'websockets relayed through this computer' };
    case 'net.proxy':
      return { icon: 'plug', title: 'tunnel TCP traffic', subtitle: 'general TCP relayed through this computer' };
    case 'geo':
      return { icon: 'pin', title: 'see your location', subtitle: 'forwarded from this computer' };
    case 'notifications':
      return { icon: 'bell', title: 'show forwarded notifications' };
    case 'audio':
    case 'audio.tts':
      return { icon: 'speaker', title: 'play sound', subtitle: 'plays through this computer' };
    case 'mic':
      return { icon: 'mic', title: 'use the Car Thing microphone' };
    default:
      return { icon: 'shield', title: permission };
  }
}
