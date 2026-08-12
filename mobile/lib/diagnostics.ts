import type { BridgethingDeviceLogLine } from '@bridgething/session-react-native';
import { useMemo } from 'react';
import { AppState } from 'react-native';
import { create } from 'zustand';
import { useShallow } from 'zustand/react/shallow';

import { getSession } from './session';

export const LOG_LIMIT = 1000;

export type LogOrigin = 'device' | 'local';

export type DeviceLogLine = {
  id: string;
  ts: number;
  level: string;
  message: string;
};

export function toLogLines(
  lines: BridgethingDeviceLogLine[],
  prefix: string,
): DeviceLogLine[] {
  return lines.map(l => ({
    id: `${prefix}${l.seq}`,
    ts: l.ts,
    level: l.level,
    message: l.message,
  }));
}

type DiagState = {
  deviceLogStreaming: boolean;
  localLogStreaming: boolean;
  logs: Record<LogOrigin, DeviceLogLine[]>;

  ingestDeviceLog(origin: string, level: string, message: string): void;
  seedLogs(origin: LogOrigin, lines: BridgethingDeviceLogLine[]): void;
  setDeviceLogStreaming(on: boolean): void;
  setLocalLogStreaming(on: boolean): void;
  clearDeviceLogs(): void;
};

let logCounter = 0;
const FLUSH_MS = 120;
let pending: Record<LogOrigin, DeviceLogLine[]> = { device: [], local: [] };
let flushTimer: ReturnType<typeof setTimeout> | null = null;

function capped(lines: DeviceLogLine[]): DeviceLogLine[] {
  return lines.length > LOG_LIMIT
    ? lines.slice(lines.length - LOG_LIMIT)
    : lines;
}

function flushPending(): void {
  flushTimer = null;
  if (pending.device.length === 0 && pending.local.length === 0) return;
  const batch = pending;
  pending = { device: [], local: [] };
  useDiagnosticsStore.setState(s => ({
    logs: {
      device: capped(s.logs.device.concat(batch.device)),
      local: capped(s.logs.local.concat(batch.local)),
    },
  }));
}

function scheduleFlush(): void {
  flushTimer ??= setTimeout(flushPending, FLUSH_MS);
}

function dropPending(origin?: LogOrigin): void {
  if (origin) {
    pending[origin] = [];
    return;
  }
  pending = { device: [], local: [] };
  if (flushTimer) {
    clearTimeout(flushTimer);
    flushTimer = null;
  }
}

export const useDiagnosticsStore = create<DiagState>((set, get) => ({
  deviceLogStreaming: false,
  localLogStreaming: false,
  logs: { device: [], local: [] },

  ingestDeviceLog: (origin, level, message) => {
    const s = get();
    if (!s.deviceLogStreaming && !s.localLogStreaming) return;
    const bucket: LogOrigin = origin === 'device' ? 'device' : 'local';
    const held = pending[bucket];
    held.push({ id: `l${logCounter++}`, ts: Date.now(), level, message });
    if (held.length > LOG_LIMIT) held.splice(0, held.length - LOG_LIMIT);
    scheduleFlush();
  },

  seedLogs: (origin, lines) => {
    dropPending(origin);
    const mapped = capped(
      toLogLines(
        lines.filter(l => l.origin === origin),
        `s${origin}-`,
      ),
    );
    set(s => ({ logs: { ...s.logs, [origin]: mapped } }));
  },

  setDeviceLogStreaming: on => {
    set({ deviceLogStreaming: on });
    getSession().setLogStreamingEnabled(on);
    if (on) {
      getSession()
        .deviceLogSnapshot(LOG_LIMIT)
        .then(lines => get().seedLogs('device', lines))
        .catch(() => {});
    }
  },

  setLocalLogStreaming: on => {
    set({ localLogStreaming: on });
    getSession().setLocalLogStreamingEnabled(on);
    if (on) void seedCurrentLaunch();
  },

  clearDeviceLogs: () => {
    dropPending();
    set({ logs: { device: [], local: [] } });
  },
}));

async function seedCurrentLaunch(): Promise<void> {
  try {
    const session = getSession();
    const current = (await session.logArchives()).find(a => a.current);
    if (!current) return;
    const lines = await session.logArchiveLines(current.id, LOG_LIMIT);
    useDiagnosticsStore.getState().seedLogs('local', lines);
  } catch {
    // a launch with nothing on disk yet just starts empty
  }
}

export function mergeLogs(
  logs: Record<LogOrigin, DeviceLogLine[]>,
): DeviceLogLine[] {
  const device = logs.device;
  const local = logs.local;
  if (device.length === 0) return local;
  if (local.length === 0) return device;
  const out: DeviceLogLine[] = [];
  let d = 0;
  let l = 0;
  while (d < device.length && l < local.length) {
    out.push(device[d].ts <= local[l].ts ? device[d++] : local[l++]);
  }
  while (d < device.length) out.push(device[d++]);
  while (l < local.length) out.push(local[l++]);
  return capped(out);
}

let wired = false;

export async function startDiagnostics(): Promise<void> {
  const session = getSession();

  if (!wired) {
    session.subscribe(event => {
      if (event.type === 'log')
        useDiagnosticsStore
          .getState()
          .ingestDeviceLog(event.origin, event.level, event.message);
    });
    AppState.addEventListener('change', next => {
      if (next !== 'active') return;
      const s = useDiagnosticsStore.getState();
      if (!s.deviceLogStreaming) return;
      session
        .deviceLogSnapshot(LOG_LIMIT)
        .then(lines => s.seedLogs('device', lines))
        .catch(() => {});
    });
    wired = true;
  }
}

export function useDiagnostics<T>(selector: (state: DiagState) => T): T {
  return useDiagnosticsStore(useShallow(selector));
}

export function useMergedLogs(): DeviceLogLine[] {
  const logs = useDiagnostics(s => s.logs);
  return useMemo(() => mergeLogs(logs), [logs]);
}
