import type { BridgethingDiagEntry } from '@bridgething/session-react-native';
import { create } from 'zustand';
import { useShallow } from 'zustand/react/shallow';

import { getSession } from './session';

const DIAG_LIMIT = 2000;
const LOG_LIMIT = 1000;

export type DeviceLogLine = {
  id: number;
  ts: number;
  level: string;
  message: string;
};

type DiagState = {
  entries: BridgethingDiagEntry[];
  logStreaming: boolean;
  deviceLogs: DeviceLogLine[];

  ingestEntry(entry: BridgethingDiagEntry): void;
  seedEntries(entries: BridgethingDiagEntry[]): void;
  ingestDeviceLog(level: string, message: string): void;
  setLogStreaming(on: boolean): void;
  clearDeviceLogs(): void;
};

let logCounter = 0;

export const useDiagnosticsStore = create<DiagState>((set, get) => ({
  entries: [],
  logStreaming: false,
  deviceLogs: [],

  ingestEntry: entry =>
    set(s => {
      // the foreground tail pull and the live stream overlap; seq de-dups.
      if (
        s.entries.length > 0 &&
        entry.seq <= s.entries[s.entries.length - 1].seq
      ) {
        if (s.entries.some(e => e.seq === entry.seq)) return s;
      }
      const next = [...s.entries, entry];
      if (next.length > DIAG_LIMIT) next.splice(0, next.length - DIAG_LIMIT);
      return { entries: next };
    }),

  seedEntries: entries =>
    set(s => {
      const bySeq = new Map<number, BridgethingDiagEntry>();
      for (const e of entries) bySeq.set(e.seq, e);
      for (const e of s.entries) bySeq.set(e.seq, e);
      const merged = [...bySeq.values()].sort((a, b) => a.seq - b.seq);
      if (merged.length > DIAG_LIMIT)
        merged.splice(0, merged.length - DIAG_LIMIT);
      return { entries: merged };
    }),

  ingestDeviceLog: (level, message) => {
    if (!get().logStreaming) return;
    set(s => {
      const next = [
        ...s.deviceLogs,
        { id: logCounter++, ts: Date.now(), level, message },
      ];
      if (next.length > LOG_LIMIT) next.splice(0, next.length - LOG_LIMIT);
      return { deviceLogs: next };
    });
  },

  setLogStreaming: on => {
    set({ logStreaming: on });
    getSession().setLogStreamingEnabled(on);
  },

  clearDeviceLogs: () => set({ deviceLogs: [] }),
}));

let wired = false;

export async function startDiagnostics(): Promise<void> {
  const session = getSession();
  const store = useDiagnosticsStore.getState();

  if (!wired) {
    session.subscribe(event => {
      if (event.type === 'diagEntry') store.ingestEntry(event.entry);
      else if (event.type === 'log')
        store.ingestDeviceLog(event.level, event.message);
    });
    wired = true;
  }

  try {
    const tail = await session.diagnosticsSnapshot(DIAG_LIMIT);
    store.seedEntries(tail);
  } catch (err) {
    console.warn('[bridgething] diagnostics tail pull failed', err);
  }
}

export function useDiagnostics<T>(selector: (state: DiagState) => T): T {
  return useDiagnosticsStore(useShallow(selector));
}
