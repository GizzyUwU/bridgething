import type {
  BridgethingActiveWebapp,
  BridgethingWebappInfo,
} from '@bridgething/session-react-native';
import { create } from 'zustand';
import { useShallow } from 'zustand/react/shallow';

import { registerDomain } from './bridge';

export type DeviceWebapps = {
  list: BridgethingWebappInfo[];
  active: BridgethingActiveWebapp | null;
};

const empty: DeviceWebapps = { list: [], active: null };

type WebappsState = {
  byDevice: Record<string, DeviceWebapps>;
};

export const useWebappsStore = create<WebappsState>(() => ({ byDevice: {} }));

export function registerWebappsDomain(): void {
  registerDomain({
    name: 'webapps',
    apply: event => {
      if (event.type === 'webappsChanged') {
        const { deviceId, webapps, active } = event.entry;
        useWebappsStore.setState(s => ({
          byDevice: {
            ...s.byDevice,
            [deviceId]: { list: webapps, active: active ?? null },
          },
        }));
        return;
      }
      if (event.type === 'peerDisconnected') {
        useWebappsStore.setState(s => {
          const next = { ...s.byDevice };
          delete next[event.peerId];
          return { byDevice: next };
        });
      }
    },
    reconcile: snapshot =>
      useWebappsStore.setState({
        byDevice: Object.fromEntries(
          snapshot.webapps.map(entry => [
            entry.deviceId,
            { list: entry.webapps, active: entry.active ?? null },
          ]),
        ),
      }),
  });
}

export function useWebapps(deviceId: string): DeviceWebapps {
  return useWebappsStore(useShallow(s => s.byDevice[deviceId] ?? empty));
}

export function installedWebapps(
  deviceId: string | null,
): BridgethingWebappInfo[] {
  if (!deviceId) return [];
  return useWebappsStore.getState().byDevice[deviceId]?.list ?? [];
}
