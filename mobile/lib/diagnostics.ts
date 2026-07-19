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
  deviceLogStreaming: boolean;
  localLogStreaming: boolean;
  deviceLogs: DeviceLogLine[];

  ingestDeviceLog(level: string, message: string): void;
  seedDeviceLogs(lines: BridgethingDeviceLogLine[]): void;
  setDeviceLogStreaming(on: boolean): void;
  setLocalLogStreaming(on: boolean): void;
  clearDeviceLogs(): void;
};

let logCounter = 0;
const FLUSH_MS = 120;
let pending: DeviceLogLine[] = [];
let flushTimer: ReturnType<typeof setTimeout> | null = null;

function flushPending(): void {
  flushTimer = null;
  if (pending.length === 0) return;
  const batch = pending;
  pending = [];
  useDiagnosticsStore.setState(s => {
    const merged = s.deviceLogs.concat(batch);
    return {
      deviceLogs:
        merged.length > LOG_LIMIT
          ? merged.slice(merged.length - LOG_LIMIT)
          : merged,
    };
  });
}

function scheduleFlush(): void {
  flushTimer ??= setTimeout(flushPending, FLUSH_MS);
}

function dropPending(): void {
  pending = [];
  if (flushTimer) {
    clearTimeout(flushTimer);
    flushTimer = null;
  }
}

export const useDiagnosticsStore = create<DiagState>((set, get) => ({
  deviceLogStreaming: false,
  localLogStreaming: false,
  deviceLogs: [],

  ingestDeviceLog: (level, message) => {
    const s = get();
    if (!s.deviceLogStreaming && !s.localLogStreaming) return;
    pending.push({ id: `l${logCounter++}`, ts: Date.now(), level, message });
    if (pending.length > LOG_LIMIT)
      pending.splice(0, pending.length - LOG_LIMIT);
    scheduleFlush();
  },

  seedDeviceLogs: lines => {
    dropPending();
    const mapped = lines.map(l => ({
      id: `s${l.seq}`,
      ts: l.ts,
      level: l.level,
      message: l.message,
    }));
    if (mapped.length > LOG_LIMIT) mapped.splice(0, mapped.length - LOG_LIMIT);
    set({ deviceLogs: mapped });
  },

  setDeviceLogStreaming: on => {
    set({ deviceLogStreaming: on });
    getSession().setLogStreamingEnabled(on);
    if (on) {
      getSession()
        .deviceLogSnapshot(LOG_LIMIT)
        .then(lines => get().seedDeviceLogs(lines))
        .catch(() => {});
    }
  },

  setLocalLogStreaming: on => {
    set({ localLogStreaming: on });
    getSession().setLocalLogStreamingEnabled(on);
  },

  clearDeviceLogs: () => {
    dropPending();
    set({ deviceLogs: [] });
  },
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
      if (!s.deviceLogStreaming) return;
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
