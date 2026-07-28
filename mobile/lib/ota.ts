import type {
  BridgethingOtaAvailable,
  BridgethingOtaPollStatus,
  BridgethingOtaRun,
  BridgethingOtaStep,
} from '@bridgething/session-react-native';
import { useEffect, useState } from 'react';
import { create } from 'zustand';
import { useShallow } from 'zustand/react/shallow';

import { getSession, registerDomain } from './bridge';

type OtaState = {
  poll: BridgethingOtaPollStatus;
  available: Record<string, BridgethingOtaAvailable>;
  runs: Record<string, BridgethingOtaRun>;
};

const empty: OtaState = { poll: {}, available: {}, runs: {} };

export const useOtaStore = create<OtaState>(() => ({ ...empty }));

export function registerOtaDomain(): void {
  registerDomain({
    name: 'ota',
    apply: event => {
      switch (event.type) {
        case 'otaRunChanged':
          useOtaStore.setState(s => ({
            runs: { ...s.runs, [event.run.deviceId]: event.run },
          }));
          return;
        case 'otaAvailableChanged':
          useOtaStore.setState(s => ({
            available: {
              ...s.available,
              [event.available.deviceId]: event.available,
            },
          }));
          return;
        case 'otaPollChanged':
          useOtaStore.setState({ poll: event.status });
          return;
        default:
          return;
      }
    },
    reconcile: snapshot =>
      useOtaStore.setState({
        poll: snapshot.otaPoll,
        available: Object.fromEntries(
          snapshot.otaAvailable.map(a => [a.deviceId, a]),
        ),
        runs: Object.fromEntries(snapshot.otaRuns.map(r => [r.deviceId, r])),
      }),
  });
}

const REBOOT_SECS = 45;
const BATCH_APPLY_SECS = 15;
const MIN_STEP_SECS = 1;

function stepSeconds(step: BridgethingOtaStep, run: BridgethingOtaRun): number {
  switch (step.kind) {
    case 'download':
    case 'stream': {
      const rate = run.ratePerSec && run.ratePerSec > 0 ? run.ratePerSec : null;
      if (!rate || step.bytes === 0) return MIN_STEP_SECS;
      return Math.max(MIN_STEP_SECS, step.bytes / rate);
    }
    case 'apply':
      return step.bytes > 0
        ? Math.max(MIN_STEP_SECS, step.bytes / 750_000)
        : BATCH_APPLY_SECS;
    case 'reboot':
      return REBOOT_SECS;
  }
}

function stepFraction(
  step: BridgethingOtaStep,
  run: BridgethingOtaRun,
  now: number,
): number {
  if (step.kind === 'reboot') {
    const elapsed = Math.max(0, now - run.phaseStartedAt) / 1000;
    return 1 - Math.exp(-elapsed / REBOOT_SECS);
  }
  if (step.kind === 'apply') {
    if (run.otaKind === 'image') {
      if (run.phase === 'confirming' || run.phase === 'reboot') return 1;
      return Math.min(1, (run.dwlPercent ?? 0) / 100);
    }
    return run.phase === 'writing' || run.phase === 'confirming' ? 1 : 0;
  }
  const total = run.stageTotal ?? 0;
  if (total > 0) return Math.min(1, (run.stageReceived ?? 0) / total);
  return 0;
}

export type OtaProgress = {
  percent: number;
  stepIndex: number;
  stepCount: number;
  stepLabel: string | null;
  etaSeconds: number | null;
};

export function otaProgress(run: BridgethingOtaRun, now: number): OtaProgress {
  const secs = run.steps.map(step => stepSeconds(step, run));
  const total = secs.reduce((a, b) => a + b, 0);
  const index = Math.max(
    0,
    run.steps.findIndex(s => s.id === run.stepId),
  );

  if (run.outcome === 'succeeded') {
    return {
      percent: 100,
      stepIndex: index,
      stepCount: run.steps.length,
      stepLabel: null,
      etaSeconds: 0,
    };
  }
  if (total <= 0) {
    return {
      percent: 0,
      stepIndex: index,
      stepCount: run.steps.length,
      stepLabel: null,
      etaSeconds: null,
    };
  }

  let elapsed = 0;
  let remaining = 0;
  run.steps.forEach((step, at) => {
    if (at < index) {
      elapsed += secs[at];
      return;
    }
    const fraction = at === index ? stepFraction(step, run, now) : 0;
    elapsed += secs[at] * fraction;
    remaining += secs[at] * (1 - fraction);
  });

  return {
    percent: Math.min(100, Math.round((elapsed / total) * 100)),
    stepIndex: index,
    stepCount: run.steps.length,
    stepLabel: run.steps[index]?.label ?? null,
    etaSeconds: Math.round(remaining),
  };
}

export function isRunning(
  run: BridgethingOtaRun | undefined,
): run is BridgethingOtaRun {
  return run !== undefined && run.outcome === undefined;
}

export function useOta<T>(selector: (state: OtaState) => T): T {
  return useOtaStore(useShallow(selector));
}

export function useOtaRun(
  deviceId: string | null,
): BridgethingOtaRun | undefined {
  return useOtaStore(s => (deviceId ? s.runs[deviceId] : undefined));
}

function useNow(active: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    const timer = setInterval(() => setNow(Date.now()), 500);
    return () => clearInterval(timer);
  }, [active]);
  return active ? now : Date.now();
}

export function useOtaProgress(
  deviceId: string | null,
): (OtaProgress & { run: BridgethingOtaRun }) | null {
  const run = useOtaRun(deviceId);
  const timed =
    run !== undefined && run.outcome === undefined && run.phase === 'reboot';
  const now = useNow(timed);
  if (!run) return null;
  return { ...otaProgress(run, now), run };
}

export function dismissOtaRun(deviceId: string): void {
  void getSession()
    .dismissOtaRun(deviceId)
    .catch(() => {});
}

export async function installLatestOta(
  deviceId: string,
  channel: string,
): Promise<void> {
  const session = getSession();
  const manifest = await session.fetchOtaManifest(null);
  const latest = manifest.channels.find(c => c.slug === channel)?.latest;
  if (latest) await session.applyOtaUpdate(deviceId, channel, latest, null);
}
