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

function phaseLabel(status: OtaDeviceStatus): string {
  switch (status.phase) {
    case 'downloading':
      return status.stageAsset
        ? `downloading ${status.stageAsset}`
        : 'downloading update';
    case 'streaming':
      return 'sending to device';
    case 'rangePull':
      return status.stageAsset
        ? `pulling delta (${status.stageAsset})`
        : 'pulling delta';
    case 'verifying':
      return 'verifying';
    case 'writing':
      return 'writing to device';
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
  const detail = stageDetail(status);
  const pct = Math.max(0, Math.min(100, status.percent));
  // range-pull has no fixed total, so show an indeterminate-ish full bar rather than a stuck 0%.
  const barPct = status.phase === 'rangePull' ? 100 : pct;
  return (
    <View className="mt-2">
      <View className="flex-row items-baseline justify-between">
        <Text className="text-[13px] font-semibold text-foreground">
          {phaseLabel(status)}
        </Text>
        {status.phase !== 'rangePull' ? (
          <Text className="text-[12px] text-muted-foreground">
            {Math.round(pct)}%
          </Text>
        ) : null}
      </View>
      <View className="mt-2 h-2 overflow-hidden rounded-full bg-muted">
        <View
          className="h-full rounded-full bg-primary"
          style={{
            width: `${barPct}%`,
            opacity: status.phase === 'rangePull' ? 0.5 : 1,
          }}
        />
      </View>
      {detail ? (
        <Text className="mt-1 text-[11px] text-muted-foreground">{detail}</Text>
      ) : null}
    </View>
  );
}

/** True when a device has anything update-related worth surfacing. Screens that
 *  only want to show OTA when it is happening (the dashboard) gate on this;
 *  the settings updates panel renders the card unconditionally. */
export function otaHasActivity(status?: OtaDeviceStatus): boolean {
  if (!status) return false;
  return (
    status.installing ||
    status.availableTo != null ||
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
  const available = status?.availableTo ?? null;
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
