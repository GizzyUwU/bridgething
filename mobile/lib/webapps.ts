import type {
  BridgethingActiveWebapp,
  BridgethingWebappInfo,
} from '@bridgething/session-react-native';
import { create } from 'zustand';
import { useShallow } from 'zustand/react/shallow';

import { getSession } from './session';

export type DeviceWebapps = {
  list: BridgethingWebappInfo[];
  active: BridgethingActiveWebapp | null;
  loading: boolean;
  error: string | null;
};

const empty: DeviceWebapps = {
  list: [],
  active: null,
  loading: false,
  error: null,
};

type WebappsState = {
  byDevice: Record<string, DeviceWebapps>;
  patch(deviceId: string, next: Partial<DeviceWebapps>): void;
  clearDevice(deviceId: string): void;
};

const useWebappsStore = create<WebappsState>(set => ({
  byDevice: {},
  patch: (deviceId, next) =>
    set(s => ({
      byDevice: {
        ...s.byDevice,
        [deviceId]: { ...(s.byDevice[deviceId] ?? empty), ...next },
      },
    })),
  clearDevice: deviceId =>
    set(s => {
      const next = { ...s.byDevice };
      delete next[deviceId];
      return { byDevice: next };
    }),
}));

export async function refreshWebapps(deviceId: string): Promise<void> {
  const store = useWebappsStore.getState();
  store.patch(deviceId, { loading: true, error: null });
  try {
    const session = getSession();
    const [list, active] = await Promise.all([
      session.listWebapps(deviceId),
      session.currentWebapp(deviceId),
    ]);
    store.patch(deviceId, { list, active, loading: false, error: null });
  } catch (err) {
    store.patch(deviceId, {
      loading: false,
      error: err instanceof Error ? err.message : String(err),
    });
  }
}

let wired = false;

export function startWebapps(): void {
  if (wired) return;
  getSession().subscribe(event => {
    if (event.type === 'peerConnected') {
      void refreshWebapps(event.peer.id);
    } else if (event.type === 'peerDisconnected') {
      useWebappsStore.getState().clearDevice(event.peerId);
    } else if (event.type === 'webappsChanged') {
      void refreshWebapps(event.deviceId);
    }
  });
  wired = true;
}

export function useWebapps(deviceId: string): DeviceWebapps {
  return useWebappsStore(useShallow(s => s.byDevice[deviceId] ?? empty));
}
