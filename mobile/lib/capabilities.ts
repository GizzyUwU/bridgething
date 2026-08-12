import type { BridgethingCapabilityFlags } from '@bridgething/session-react-native';

import type { IconName } from '../components/Icon';

export type CapabilityKey = keyof BridgethingCapabilityFlags;

export type CapabilityGroup = 'connections' | 'sharing' | 'voice';

export type CapabilityCopy = {
  group: CapabilityGroup;
  icon: IconName;
  title: string;
  subtitle: string;
};

export const CAPABILITY_GROUPS: CapabilityGroup[] = [
  'connections',
  'sharing',
  'voice',
];

export const CAPABILITIES: Record<CapabilityKey, CapabilityCopy> = {
  notifications: {
    group: 'connections',
    icon: 'Bell',
    title: 'notifications',
    subtitle: 'send what your phone shows you to the car thing',
  },
  geo: {
    group: 'sharing',
    icon: 'MapPin',
    title: 'location',
    subtitle: 'let apps on your car thing see where you are',
  },
  netFetch: {
    group: 'sharing',
    icon: 'Globe',
    title: 'internet access',
    subtitle: 'apps on your car thing load data through this phone',
  },
  netWs: {
    group: 'sharing',
    icon: 'Wifi',
    title: 'live updates',
    subtitle: 'apps keep a live connection open through this phone',
  },
  audioTts: {
    group: 'sharing',
    icon: 'Speaker',
    title: 'phone speaker',
    subtitle: 'let your car thing play sound through this phone',
  },
  voiceModel: {
    group: 'voice',
    icon: 'Mic',
    title: 'voice understanding',
    subtitle: 'handle free-form requests, not just the built-in phrases',
  },
};

export const CAPABILITY_KEYS = Object.keys(CAPABILITIES) as CapabilityKey[];

export function capabilitiesIn(group: CapabilityGroup): CapabilityKey[] {
  return CAPABILITY_KEYS.filter(key => CAPABILITIES[key].group === group);
}
