import type {
  BridgethingOtaAvailable,
  BridgethingOtaRun,
} from '@bridgething/session-react-native';
import { ChevronRight, X } from 'lucide-react-native';
import { Text, View } from 'react-native';

import { Button } from './Button';
import { Press } from './Press';
import { dismissOtaRun, useOtaProgress, type OtaProgress } from '../lib/ota';

function formatBytes(n: number): string {
  if (n < 1024) return `${Math.round(n)} B`;
  if (n < 1024 * 1024) return `${Math.round(n / 1024)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function formatEta(seconds: number): string {
  const s = Math.round(seconds);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  const r = s % 60;
  return r ? `${m}m ${r}s` : `${m}m`;
}

function isPullingDeltas(run: BridgethingOtaRun): boolean {
  return run.dwlPercent != null && run.dwlPercent < 100;
}

function phaseLabel(run: BridgethingOtaRun, stepLabel: string | null): string {
  switch (run.phase) {
    case 'downloading':
      return stepLabel ? `downloading ${stepLabel}` : 'downloading update';
    case 'streaming':
      return stepLabel ? `${stepLabel} to device` : 'sending to device';
    case 'verifying':
      return 'verifying';
    case 'writing':
      return isPullingDeltas(run) ? 'pulling deltas' : 'writing to device';
    case 'confirming':
      return 'confirming';
    case 'reboot':
      return 'rebooting device';
    default:
      return 'preparing update';
  }
}

function stageDetail(
  run: BridgethingOtaRun,
  etaSeconds: number | null,
): string | null {
  const parts: string[] = [];
  if (run.stageReceived != null) {
    parts.push(
      run.stageTotal != null && run.stageTotal > 0
        ? `${formatBytes(run.stageReceived)} / ${formatBytes(run.stageTotal)}`
        : formatBytes(run.stageReceived),
    );
  }
  if (run.ratePerSec != null && run.ratePerSec > 0)
    parts.push(`${formatBytes(run.ratePerSec)}/s`);
  if (etaSeconds != null && etaSeconds > 0)
    parts.push(`${formatEta(etaSeconds)} left`);
  if (parts.length === 0 && run.phase === 'writing' && !isPullingDeltas(run)) {
    return 'this can take several minutes';
  }
  return parts.length ? parts.join(' · ') : null;
}

function UpdateProgress({
  run,
  progress,
}: {
  run: BridgethingOtaRun;
  progress: OtaProgress;
}) {
  const detail = stageDetail(run, progress.etaSeconds);
  return (
    <View className="mt-2">
      <View className="flex-row items-baseline justify-between">
        <Text className="text-[13px] font-semibold text-foreground">
          {phaseLabel(run, progress.stepLabel)}
        </Text>
        <Text className="text-[12px] text-muted-foreground">
          {progress.percent}%
        </Text>
      </View>
      <View className="mt-2 h-2 overflow-hidden rounded-full bg-muted">
        <View
          className="h-full rounded-full bg-primary"
          style={{ width: `${progress.percent}%` }}
        />
      </View>
      <View className="mt-1 flex-row items-baseline justify-between">
        <Text className="text-[11px] text-muted-foreground">
          {detail ?? ''}
        </Text>
        {progress.stepCount > 0 ? (
          <Text className="text-[11px] text-muted-foreground">
            step {progress.stepIndex + 1}/{progress.stepCount}
          </Text>
        ) : null}
      </View>
    </View>
  );
}

function releaseLabel(
  available: BridgethingOtaAvailable | undefined,
): string | null {
  if (!available) return null;
  if (available.daemonVersion && available.imageVersion) {
    return `daemon ${available.daemonVersion} · image ${available.imageVersion}`;
  }
  return available.releaseVersion ?? null;
}

export function otaHasActivity(
  run?: BridgethingOtaRun,
  available?: BridgethingOtaAvailable,
): boolean {
  return run !== undefined || releaseLabel(available) !== null;
}

export function OtaCard({
  deviceId,
  name,
  available,
  onInstall,
  onPickVersion,
}: {
  deviceId: string;
  name: string;
  available?: BridgethingOtaAvailable;
  onInstall?: () => void;
  onPickVersion?: () => void;
}) {
  const progress = useOtaProgress(deviceId);
  const run = progress?.run;
  const offer = releaseLabel(available);
  const title = run?.webappName ?? name;

  return (
    <View className="mt-3 rounded-2xl border border-border bg-surface p-4">
      <View className="flex-row items-center justify-between">
        <Text className="text-[14px] font-semibold text-foreground">
          {title}
        </Text>
        {run?.outcome ? (
          <Press
            onPress={() => dismissOtaRun(deviceId)}
            scaleTo={0.9}
            hitSlop={10}
          >
            <X size={16} color="hsl(215 14% 50%)" strokeWidth={2.4} />
          </Press>
        ) : null}
      </View>

      {progress && run && !run.outcome ? (
        <UpdateProgress run={run} progress={progress} />
      ) : run?.outcome === 'succeeded' ? (
        <Text className="mt-1 text-[12px] text-muted-foreground">
          update installed
        </Text>
      ) : run?.outcome === 'cancelled' ? (
        <Text className="mt-1 text-[12px] text-muted-foreground">
          update cancelled
        </Text>
      ) : offer ? (
        <View className="mt-2">
          <Text className="mb-2 text-[12px] text-muted-foreground">
            update available: {offer}
          </Text>
          {onInstall ? (
            <Button onPress={onInstall} size="md">
              install update
            </Button>
          ) : null}
        </View>
      ) : run?.outcome === 'failed' ? (
        <Text className="mt-1 text-[12px] text-muted-foreground">
          update failed
        </Text>
      ) : (
        <Text className="mt-1 text-[12px] text-muted-foreground">
          up to date
        </Text>
      )}

      {run?.error ? (
        <Text className="mt-2 text-[12px] text-destructive">{run.error}</Text>
      ) : null}

      {onPickVersion ? (
        <Press
          onPress={onPickVersion}
          scaleTo={0.99}
          fade={false}
          className="mt-3 flex-row items-center gap-1 py-1"
        >
          <Text className="text-[13px] font-semibold text-primary">
            choose a specific version
          </Text>
          <ChevronRight size={14} color="hsl(215 14% 50%)" strokeWidth={2.4} />
        </Press>
      ) : null}
    </View>
  );
}
