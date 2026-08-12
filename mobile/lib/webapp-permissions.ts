import type { IconName } from '../components/Icon';

export type PermissionCopy = {
  icon: IconName;
  title: string;
  subtitle?: string;
};

export function humanizePermission(perm: string): PermissionCopy {
  switch (perm) {
    case 'net.fetch':
      return {
        icon: 'Globe',
        title: 'use the internet',
        subtitle: 'data fetched via your phone',
      };
    case 'net.ws':
      return {
        icon: 'Wifi',
        title: 'real-time data',
        subtitle: 'websockets via your phone',
      };
    case 'net.proxy':
      return {
        icon: 'Cable',
        title: 'tunnel TCP traffic',
        subtitle: 'general TCP via your phone',
      };
    case 'geo':
      return {
        icon: 'MapPin',
        title: 'see your location',
        subtitle: 'forwarded from your phone',
      };
    case 'notifications':
      return { icon: 'Bell', title: 'show phone notifications' };
    case 'audio.tts':
    case 'audio':
      return {
        icon: 'Speaker',
        title: 'play sound',
        subtitle: 'plays through your phone',
      };
    case 'mic':
      return { icon: 'Mic', title: 'use the Car Thing microphone' };
    default:
      return { icon: 'Shield', title: perm };
  }
}
