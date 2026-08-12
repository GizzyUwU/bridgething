import type {
  BridgethingOtaProgress,
  BridgethingOtaRun,
} from '@bridgething/session-react-native';
import { Text, View } from 'react-native';

import { Progress } from './Progress';
import { TEXT } from '../lib/theme';
import { formatBytes } from '../lib/utils';

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

export function otaPhaseLabel(
  run: BridgethingOtaRun,
  stepLabel?: string,
): string {
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
  etaSeconds?: number,
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

export function OtaRunProgress({
  run,
  progress,
  className,
}: {
  run: BridgethingOtaRun;
  progress: BridgethingOtaProgress;
  className?: string;
}) {
  const detail = stageDetail(run, progress.etaSeconds);
  return (
    <View className={className}>
      <View className="flex-row items-baseline justify-between">
        <Text className="font-sans text-fg" style={TEXT.hint}>
          {otaPhaseLabel(run, progress.stepLabel)}
        </Text>
        <Text className="font-mono text-soft" style={TEXT.hint}>
          {progress.percent}%
        </Text>
      </View>
      <Progress percent={progress.percent} className="mt-2" />
      <View className="mt-1.5 flex-row items-baseline justify-between">
        <Text className="font-mono text-dim" style={TEXT.eyebrow}>
          {detail ?? ''}
        </Text>
        {progress.stepCount > 0 ? (
          <Text className="font-mono text-dim" style={TEXT.eyebrow}>
            step {progress.stepIndex + 1}/{progress.stepCount}
          </Text>
        ) : null}
      </View>
    </View>
  );
}

export function OtaStarting({ className }: { className?: string }) {
  return (
    <View className={className}>
      <Text className="font-sans text-fg" style={TEXT.hint}>
        asking your car thing to start
      </Text>
      <Progress percent={null} className="mt-2" />
    </View>
  );
}
