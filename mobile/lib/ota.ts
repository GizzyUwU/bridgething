import type {
  BridgethingOtaEvent,
  BridgethingOtaKind,
  BridgethingOtaPhase,
  BridgethingOtaStep,
} from '@bridgething/session-react-native';
import { create } from 'zustand';
import { useShallow } from 'zustand/react/shallow';

import { getSession } from './session';

const DOWNLOAD_BYTES_PER_SEC = 8_000_000; // wifi to R2, conservative
const STREAM_BYTES_PER_SEC = 8_000; // background-priority BT fragment stream, measured on the EA link
const APPLY_BYTES_PER_SEC = 750_000; // device apply, weighted by zck: BT delta-pull + eMMC write bound
const BATCH_APPLY_SECS = 15; // activate + restart for a bandaid batch
const REBOOT_SECS = 25;
const MIN_STEP_SECS = 1; // floor so a zero-byte leg still occupies a sliver of the bar
const IMAGE_APPLY_PULL_SLICE = 0.95;

function stepSeconds(step: BridgethingOtaStep): number {
  switch (step.kind) {
    case 'download':
      return Math.max(MIN_STEP_SECS, step.bytes / DOWNLOAD_BYTES_PER_SEC);
    case 'stream':
      return Math.max(MIN_STEP_SECS, step.bytes / STREAM_BYTES_PER_SEC);
    case 'apply':
      return step.bytes > 0
        ? step.bytes / APPLY_BYTES_PER_SEC
        : BATCH_APPLY_SECS;
    case 'reboot':
      return REBOOT_SECS;
  }
}

export type OtaDeviceStatus = {
  phase: BridgethingOtaPhase;
  otaKind: BridgethingOtaKind | null;
  installing: boolean;
  error: string | null;

  overallPercent: number;
  stepIndex: number;
  stepCount: number;
  stepLabel: string | null;

  availableRelease: string | null;
  availableDaemon: string | null;
  availableImage: string | null;

  stageReceived: number | null;
  stageTotal: number | null;
  stageRatePerSec: number | null;
  stageEtaSeconds: number | null;
  dwlPercent: number | null;

  plan: BridgethingOtaStep[];
  stepSecs: number[];
  totalSecs: number;

  sampleBytes: number | null;
  sampleAt: number | null;
};

const idleStatus: OtaDeviceStatus = {
  phase: 'idle',
  otaKind: null,
  installing: false,
  error: null,
  overallPercent: 0,
  stepIndex: 0,
  stepCount: 0,
  stepLabel: null,
  availableRelease: null,
  availableDaemon: null,
  availableImage: null,
  stageReceived: null,
  stageTotal: null,
  stageRatePerSec: null,
  stageEtaSeconds: null,
  dwlPercent: null,
  plan: [],
  stepSecs: [],
  totalSecs: 0,
  sampleBytes: null,
  sampleAt: null,
};

const clearedStage = {
  stageReceived: null,
  stageTotal: null,
  stageRatePerSec: null,
  stageEtaSeconds: null,
  dwlPercent: null,
  sampleBytes: null,
  sampleAt: null,
} as const;

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

function stepFraction(
  step: BridgethingOtaStep,
  event: BridgethingOtaEvent,
  otaKind: BridgethingOtaKind | null,
): number {
  if (step.kind === 'reboot') return 0; // device is gone; the bar lands on 100 at `updated`.
  if (step.kind === 'apply') {
    if (otaKind === 'image') {
      const dwl = event.dwlPercent ?? 0;
      if (event.phase === 'confirming' || event.phase === 'reboot') return 1;
      return (Math.min(dwl, 100) / 100) * IMAGE_APPLY_PULL_SLICE;
    }
    return Math.min(1, (event.percent ?? 0) / 100);
  }
  const total = event.stageTotal ?? 0;
  const received = event.stageReceived ?? 0;
  if ((step.kind === 'download' || step.kind === 'stream') && total > 0) {
    return Math.min(1, received / total);
  }
  return Math.min(1, (event.percent ?? 0) / 100);
}

function overallFromEvent(
  prev: OtaDeviceStatus,
  event: BridgethingOtaEvent,
): number {
  if (prev.plan.length === 0 || prev.totalSecs <= 0) {
    return Math.max(prev.overallPercent, event.percent ?? 0);
  }
  const stepId = event.stepId ?? 0;
  const step = prev.plan.find(s => s.id === stepId) ?? prev.plan[0];
  const kind = event.otaKind ?? prev.otaKind;
  let elapsed = 0;
  for (const s of prev.plan) {
    if (s.id < stepId) elapsed += prev.stepSecs[s.id] ?? 0;
    else if (s.id === stepId)
      elapsed += (prev.stepSecs[s.id] ?? 0) * stepFraction(step, event, kind);
  }
  const computed = Math.min(100, (elapsed / prev.totalSecs) * 100);
  return Math.max(prev.overallPercent, computed); // never walk backward across a leg reset.
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
            availableRelease: event.releaseVersion ?? event.toVersion ?? null,
            availableDaemon: event.daemonVersion ?? null,
            availableImage: event.imageVersion ?? null,
            error: null,
          }),
        }));
        return;
      case 'planned': {
        if (!id) return;
        const plan = event.steps ?? [];
        const stepSecs = plan.map(stepSeconds);
        const totalSecs = stepSecs.reduce((a, b) => a + b, 0);
        set(s => ({
          byDevice: patch(s, id, {
            otaKind: event.otaKind ?? null,
            installing: true,
            error: null,
            overallPercent: 0,
            stepIndex: 0,
            stepCount: plan.length,
            stepLabel: plan[0]?.label ?? null,
            availableRelease: event.releaseVersion ?? null,
            availableDaemon: event.daemonVersion ?? null,
            availableImage: event.imageVersion ?? null,
            plan,
            stepSecs,
            totalSecs,
            ...clearedStage,
          }),
        }));
        return;
      }
      case 'progress':
        if (!id) return;
        set(s => {
          const prev = s.byDevice[id] ?? idleStatus;
          const stepId = event.stepId ?? 0;
          const step = prev.plan.find(p => p.id === stepId);
          const stepIndex = step ? prev.plan.indexOf(step) : prev.stepIndex;
          const sameStep = stepIndex === prev.stepIndex;
          const kind = event.otaKind ?? prev.otaKind;
          const dwl = event.dwlPercent ?? (sameStep ? prev.dwlPercent : null);

          let stageReceived =
            event.stageReceived ?? (sameStep ? prev.stageReceived : null);
          let stageTotal =
            event.stageTotal ?? (sameStep ? prev.stageTotal : null);
          let stageRatePerSec =
            event.stageRatePerSec ?? (sameStep ? prev.stageRatePerSec : null);
          let stageEtaSeconds =
            event.stageEtaSeconds ?? (sameStep ? prev.stageEtaSeconds : null);
          let sampleBytes = sameStep ? prev.sampleBytes : null;
          let sampleAt = sameStep ? prev.sampleAt : null;

          if (step?.kind === 'apply' && kind === 'image') {
            if (dwl != null && dwl < 100 && event.stageReceived != null) {
              const now = Date.now();
              stageTotal =
                dwl > 0 ? Math.round((event.stageReceived / dwl) * 100) : null;
              if (
                sampleBytes != null &&
                sampleAt != null &&
                now > sampleAt &&
                event.stageReceived > sampleBytes
              ) {
                const inst =
                  ((event.stageReceived - sampleBytes) / (now - sampleAt)) *
                  1000;
                stageRatePerSec =
                  stageRatePerSec != null
                    ? stageRatePerSec * 0.6 + inst * 0.4
                    : inst;
                stageEtaSeconds =
                  stageTotal != null && stageRatePerSec > 0
                    ? (stageTotal - event.stageReceived) / stageRatePerSec
                    : null;
              }
              sampleBytes = event.stageReceived;
              sampleAt = now;
            } else if (dwl != null && dwl >= 100) {
              stageReceived = null;
              stageTotal = null;
              stageRatePerSec = null;
              stageEtaSeconds = null;
              sampleBytes = null;
              sampleAt = null;
            }
          }

          return {
            byDevice: patch(s, id, {
              otaKind: kind,
              phase: event.phase ?? 'streaming',
              installing: true,
              error: null,
              overallPercent: overallFromEvent(prev, event),
              stepIndex,
              stepLabel: step?.label ?? prev.stepLabel,
              stageReceived,
              stageTotal,
              stageRatePerSec,
              stageEtaSeconds,
              dwlPercent: dwl,
              sampleBytes,
              sampleAt,
            }),
          };
        });
        return;
      case 'updated':
        if (!id) return;
        set(s => ({
          byDevice: patch(s, id, {
            phase: 'completed',
            overallPercent: 100,
            installing: false,
            availableRelease: null,
            availableDaemon: null,
            availableImage: null,
            ...clearedStage,
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
            ...clearedStage,
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

export async function installLatestOta(
  deviceId: string,
  channel: string,
): Promise<void> {
  const session = getSession();
  const manifest = await session.fetchOtaManifest(null);
  const latest = manifest.channels.find(c => c.slug === channel)?.latest;
  if (latest) {
    await session.applyOtaUpdate(deviceId, channel, latest, null);
  }
}
