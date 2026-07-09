import { ChevronRight } from 'lucide-react-native';
import { Text, View } from 'react-native';

import { Button } from './Button';
import { Press } from './Press';
import type { OtaDeviceStatus } from '../lib/ota';

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

function isPullingDeltas(status: OtaDeviceStatus): boolean {
  return status.dwlPercent != null && status.dwlPercent < 100;
}

function phaseLabel(status: OtaDeviceStatus): string {
  const leg = status.stepLabel;
  switch (status.phase) {
    case 'downloading':
      return leg ? `downloading ${leg}` : 'downloading update';
    case 'streaming':
      return leg ? `${leg} to device` : 'sending to device';
    case 'verifying':
      return 'verifying';
    case 'writing':
      return isPullingDeltas(status) ? 'pulling deltas' : 'writing to device';
    case 'confirming':
      return 'confirming';
    case 'reboot':
      return 'rebooting device';
    case 'completed':
      return 'done';
    case 'failed':
      return 'failed';
    default:
      return 'preparing update';
  }
}

function phaseHint(status: OtaDeviceStatus): string | null {
  if (status.phase === 'writing' && !isPullingDeltas(status)) {
    return 'this can take several minutes';
  }
  return null;
}

function releaseLabel(status: OtaDeviceStatus): string | null {
  if (status.availableDaemon && status.availableImage) {
    return `daemon ${status.availableDaemon} · image ${status.availableImage}`;
  }
  return status.availableRelease;
}

function stageDetail(status: OtaDeviceStatus): string | null {
  const parts: string[] = [];
  if (status.stageReceived != null) {
    parts.push(
      status.stageTotal != null && status.stageTotal > 0
        ? `${formatBytes(status.stageReceived)} / ${formatBytes(status.stageTotal)}`
        : formatBytes(status.stageReceived),
    );
  }
  if (status.stageRatePerSec != null && status.stageRatePerSec > 0) {
    parts.push(`${formatBytes(status.stageRatePerSec)}/s`);
  }
  if (status.stageEtaSeconds != null && status.stageEtaSeconds > 0) {
    parts.push(`${formatEta(status.stageEtaSeconds)} left`);
  }
  return parts.length ? parts.join(' · ') : null;
}

function UpdateProgress({ status }: { status: OtaDeviceStatus }) {
  const detail = stageDetail(status) ?? phaseHint(status);
  const pct = Math.max(0, Math.min(100, status.overallPercent));
  const stepHint =
    status.stepCount > 0
      ? `step ${status.stepIndex + 1}/${status.stepCount}`
      : null;
  return (
    <View className="mt-2">
      <View className="flex-row items-baseline justify-between">
        <Text className="text-[13px] font-semibold text-foreground">
          {phaseLabel(status)}
        </Text>
        <Text className="text-[12px] text-muted-foreground">
          {Math.round(pct)}%
        </Text>
      </View>
      <View className="mt-2 h-2 overflow-hidden rounded-full bg-muted">
        <View
          className="h-full rounded-full bg-primary"
          style={{ width: `${pct}%` }}
        />
      </View>
      <View className="mt-1 flex-row items-baseline justify-between">
        <Text className="text-[11px] text-muted-foreground">
          {detail ?? ''}
        </Text>
        {stepHint ? (
          <Text className="text-[11px] text-muted-foreground">{stepHint}</Text>
        ) : null}
      </View>
    </View>
  );
}

export function otaHasActivity(status?: OtaDeviceStatus): boolean {
  if (!status) return false;
  return (
    status.installing ||
    status.availableRelease != null ||
    status.phase === 'completed' ||
    status.error != null
  );
}

export function OtaCard({
  name,
  status,
  onInstall,
  onPickVersion,
}: {
  name: string;
  status?: OtaDeviceStatus;
  onInstall?: () => void;
  onPickVersion?: () => void;
}) {
  const available = status ? releaseLabel(status) : null;
  const installing = status?.installing ?? false;

  return (
    <View className="mt-3 rounded-2xl border border-border bg-surface p-4">
      <Text className="text-[14px] font-semibold text-foreground">{name}</Text>
      {installing && status ? (
        <UpdateProgress status={status} />
      ) : available ? (
        <View className="mt-2">
          <Text className="mb-2 text-[12px] text-muted-foreground">
            update available: {available}
          </Text>
          {onInstall ? (
            <Button onPress={onInstall} size="md">
              install update
            </Button>
          ) : null}
        </View>
      ) : status?.phase === 'completed' ? (
        <Text className="mt-1 text-[12px] text-muted-foreground">
          rebooting to complete installation...
        </Text>
      ) : (
        <Text className="mt-1 text-[12px] text-muted-foreground">
          up to date
        </Text>
      )}
      {status?.error ? (
        <Text className="mt-2 text-[12px] text-destructive">
          {status.error}
        </Text>
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
