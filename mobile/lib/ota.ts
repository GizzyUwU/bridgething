import type {
  BridgethingOtaEvent,
  BridgethingOtaKind,
  BridgethingOtaPhase,
} from '@bridgething/session-react-native';
import { create } from 'zustand';
import { useShallow } from 'zustand/react/shallow';

import { getSession } from './session';

export type OtaDeviceStatus = {
  phase: BridgethingOtaPhase;
  percent: number;
  otaKind: BridgethingOtaKind | null;
  availableFrom: string | null;
  availableTo: string | null;
  installing: boolean;
  error: string | null;
};

const idleStatus: OtaDeviceStatus = {
  phase: 'idle',
  percent: 0,
  otaKind: null,
  availableFrom: null,
  availableTo: null,
  installing: false,
  error: null,
};

type OtaState = {
  lastPolledAt: string | null;
  pollError: string | null;
  byDevice: Record<string, OtaDeviceStatus>;

  ingest(event: BridgethingOtaEvent): void;
  clearDevice(deviceId: string): void;
};

function patch(
  s: OtaState,
  deviceId: string,
  next: Partial<OtaDeviceStatus>,
): Record<string, OtaDeviceStatus> {
  const prev = s.byDevice[deviceId] ?? idleStatus;
  return { ...s.byDevice, [deviceId]: { ...prev, ...next } };
}

export const useOtaStore = create<OtaState>(set => ({
  lastPolledAt: null,
  pollError: null,
  byDevice: {},

  ingest: event => {
    const id = event.deviceId;
    switch (event.kind) {
      case 'manifestPolled':
        set({ lastPolledAt: event.updatedAt ?? null, pollError: null });
        return;
      case 'manifestPollFailed':
        set({ pollError: event.reason ?? 'manifest poll failed' });
        return;
      case 'channelMismatch':
        if (!id) return;
        set(s => ({
          byDevice: patch(s, id, {
            error: `device on '${event.deviceChannel}', app set to '${event.configuredChannel}'`,
          }),
        }));
        return;
      case 'updateAvailable':
        if (!id) return;
        set(s => ({
          byDevice: patch(s, id, {
            otaKind: event.otaKind ?? null,
            availableFrom: event.fromVersion ?? null,
            availableTo: event.toVersion ?? null,
            error: null,
          }),
        }));
        return;
      case 'progress':
        if (!id) return;
        set(s => ({
          byDevice: patch(s, id, {
            otaKind: event.otaKind ?? null,
            phase: event.phase ?? 'streaming',
            percent: event.percent ?? 0,
            installing: true,
            error: null,
          }),
        }));
        return;
      case 'updated':
        if (!id) return;
        set(s => ({
          byDevice: patch(s, id, {
            phase: 'completed',
            percent: 100,
            installing: false,
            availableFrom: null,
            availableTo: null,
          }),
        }));
        return;
      case 'failed':
        if (!id) return;
        set(s => ({
          byDevice: patch(s, id, {
            phase: 'failed',
            installing: false,
            error: event.reason ?? 'update failed',
          }),
        }));
        return;
    }
  },

  clearDevice: deviceId =>
    set(s => {
      const next = { ...s.byDevice };
      delete next[deviceId];
      return { byDevice: next };
    }),
}));

let wired = false;

/** Subscribe the OTA store to the native event stream once. Idempotent across
 *  hot reloads. Called from bootstrapSession after the session starts. */
export function startOta(): void {
  if (wired) return;
  getSession().subscribe(event => {
    if (event.type === 'otaEvent') useOtaStore.getState().ingest(event.event);
  });
  wired = true;
}

export function useOta<T>(selector: (state: OtaState) => T): T {
  return useOtaStore(useShallow(selector));
}
