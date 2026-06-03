import type { BridgethingDeviceLogLine } from '@bridgething/session-react-native';
import { AppState } from 'react-native';
import { create } from 'zustand';
import { useShallow } from 'zustand/react/shallow';

import { getSession } from './session';

const LOG_LIMIT = 1000;

export type DeviceLogLine = {
  id: string;
  ts: number;
  level: string;
  message: string;
};

type DiagState = {
  logStreaming: boolean;
  deviceLogs: DeviceLogLine[];

  ingestDeviceLog(level: string, message: string): void;
  seedDeviceLogs(lines: BridgethingDeviceLogLine[]): void;
  setLogStreaming(on: boolean): void;
  clearDeviceLogs(): void;
};

let logCounter = 0;

export const useDiagnosticsStore = create<DiagState>((set, get) => ({
  logStreaming: false,
  deviceLogs: [],

  ingestDeviceLog: (level, message) => {
    if (!get().logStreaming) return;
    set(s => {
      const next = [
        ...s.deviceLogs,
        { id: `l${logCounter++}`, ts: Date.now(), level, message },
      ];
      if (next.length > LOG_LIMIT) next.splice(0, next.length - LOG_LIMIT);
      return { deviceLogs: next };
    });
  },

  seedDeviceLogs: lines => {
    const mapped = lines.map(l => ({
      id: `s${l.seq}`,
      ts: l.ts,
      level: l.level,
      message: l.message,
    }));
    if (mapped.length > LOG_LIMIT) mapped.splice(0, mapped.length - LOG_LIMIT);
    set({ deviceLogs: mapped });
  },

  setLogStreaming: on => {
    set({ logStreaming: on });
    getSession().setLogStreamingEnabled(on);
    if (on) {
      getSession()
        .deviceLogSnapshot(LOG_LIMIT)
        .then(lines => get().seedDeviceLogs(lines))
        .catch(() => {});
    }
  },

  clearDeviceLogs: () => set({ deviceLogs: [] }),
}));

let wired = false;

export async function startDiagnostics(): Promise<void> {
  const session = getSession();

  if (!wired) {
    session.subscribe(event => {
      if (event.type === 'log')
        useDiagnosticsStore
          .getState()
          .ingestDeviceLog(event.level, event.message);
    });
    AppState.addEventListener('change', next => {
      if (next !== 'active') return;
      const s = useDiagnosticsStore.getState();
      if (!s.logStreaming) return;
      session
        .deviceLogSnapshot(LOG_LIMIT)
        .then(lines => s.seedDeviceLogs(lines))
        .catch(() => {});
    });
    wired = true;
  }
}

export function useDiagnostics<T>(selector: (state: DiagState) => T): T {
  return useDiagnosticsStore(useShallow(selector));
}
