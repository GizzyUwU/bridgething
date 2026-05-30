import type {
  BridgethingCompanionDebug,
  BridgethingDiagEntry,
} from '@bridgething/session-react-native';
import type { NativeStackScreenProps } from '@react-navigation/native-stack';
import { ArrowDownLeft, ArrowUpRight, RefreshCw } from 'lucide-react-native';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { FlatList, type ListRenderItemInfo, Text, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { Press } from '../components/Press';
import { Segmented } from '../components/Segmented';
import { useDiagnostics } from '../lib/diagnostics';
import { getSession } from '../lib/session';
import type { RootStackParamList } from '../navigation';

type Props = NativeStackScreenProps<RootStackParamList, 'Debug'>;

const TABS = ['timeline', 'frames', 'state'] as const;
type Tab = (typeof TABS)[number];

export function DebugScreen({}: Props) {
  const entries = useDiagnostics(s => s.entries);
  const [tab, setTab] = useState<Tab>('timeline');
  const [companion, setCompanion] = useState<BridgethingCompanionDebug | null>(
    null,
  );

  const refresh = useCallback(async () => {
    try {
      setCompanion(await getSession().companionDebug());
    } catch {
      setCompanion(null);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const merges = useMemo(
    () =>
      entries
        .filter(
          e => e.kind === 'breadcrumb' && e.category?.startsWith('spotify'),
        )
        .slice()
        .reverse(),
    [entries],
  );
  const frames = useMemo(
    () =>
      entries
        .filter(e => e.kind === 'frame')
        .slice()
        .reverse(),
    [entries],
  );

  return (
    <SafeAreaView edges={['bottom']} className="flex-1 bg-background">
      <View className="border-b border-border bg-surface px-5 py-3">
        <View className="flex-row items-center justify-between">
          <Segmented options={TABS} value={tab} onChange={setTab} size="sm" />
          <Press
            onPress={refresh}
            scaleTo={0.92}
            className="flex-row items-center gap-1.5 rounded-full bg-primary-soft px-2.5 py-1"
          >
            <RefreshCw size={12} color="hsl(199 100% 44%)" strokeWidth={2.4} />
            <Text className="text-[11px] font-bold uppercase tracking-[0.14em] text-primary">
              refresh
            </Text>
          </Press>
        </View>
      </View>

      {tab === 'timeline' ? (
        <FlatList
          data={merges}
          keyExtractor={e => String(e.seq)}
          renderItem={renderMerge}
          ListEmptyComponent={<Empty label="no augmentation activity yet" />}
          contentContainerClassName="px-3 py-2"
        />
      ) : tab === 'frames' ? (
        <FlatList
          data={frames}
          keyExtractor={e => String(e.seq)}
          renderItem={renderFrame}
          ListEmptyComponent={<Empty label="no wire frames yet" />}
          contentContainerClassName="px-3 py-2"
        />
      ) : (
        <CompanionState companion={companion} />
      )}
    </SafeAreaView>
  );
}

function CompanionState({
  companion,
}: {
  companion: BridgethingCompanionDebug | null;
}) {
  if (!companion) {
    return <Empty label="no companion state (sign in + connect)" />;
  }
  const rows: [string, string][] = [
    ['authority · playback', yesno(companion.authorityPlaybackHeld)],
    ['authority · metadata', yesno(companion.authorityMetadataHeld)],
    ['baseline poll', companion.baselinePollActive ? 'running' : 'idle'],
    ['hint fetch', companion.hintFetchActive ? 'in flight' : 'idle'],
    ['ANCS auth', companion.ancsAuthStatus],
  ];
  return (
    <View className="px-5 py-4">
      {rows.map(([label, value]) => (
        <View
          key={label}
          className="flex-row items-center justify-between border-b border-border/50 py-2.5"
        >
          <Text className="text-[13px] text-muted-foreground">{label}</Text>
          <Text className="font-mono text-[13px] font-semibold text-foreground">
            {value}
          </Text>
        </View>
      ))}
    </View>
  );
}

function renderMerge({ item }: ListRenderItemInfo<BridgethingDiagEntry>) {
  const fields = fieldMap(item);
  const source = fields.source ?? item.category ?? '';
  return (
    <View className="border-b border-border/50 py-1.5">
      <View className="flex-row items-center gap-2">
        <View className="rounded-md bg-primary-soft px-1.5 py-0.5">
          <Text className="font-mono text-[10px] font-bold uppercase text-primary">
            {source}
          </Text>
        </View>
        <Text className="font-mono text-[10px] text-muted-foreground">
          {formatTime(item.ts)}
        </Text>
        {fields.reason ? (
          <Text className="font-mono text-[10px] text-muted-foreground">
            · {fields.reason}
          </Text>
        ) : null}
      </View>
      <Text className="mt-1 font-mono text-[12px] leading-[16px] text-foreground">
        {item.detail}
        {fields.track ? ` — ${fields.track}` : ''}
        {fields.playing
          ? ` (${fields.playing === 'true' ? 'playing' : 'paused'})`
          : ''}
      </Text>
    </View>
  );
}

function renderFrame({ item }: ListRenderItemInfo<BridgethingDiagEntry>) {
  const outbound = item.direction === 'outbound';
  const Icon = outbound ? ArrowUpRight : ArrowDownLeft;
  const tint = outbound ? 'hsl(199 100% 44%)' : 'hsl(150 50% 42%)';
  return (
    <View className="flex-row items-center gap-2 border-b border-border/50 py-1.5">
      <Icon size={13} color={tint} strokeWidth={2.6} />
      <Text className="font-mono text-[10px] text-muted-foreground">
        {formatTime(item.ts)}
      </Text>
      <Text
        className="flex-1 font-mono text-[12px] font-semibold text-foreground"
        numberOfLines={1}
      >
        {item.surface}
      </Text>
      <Text className="font-mono text-[10px] text-muted-foreground">
        {item.frameKind}
      </Text>
      {item.latencyMs != null ? (
        <Text className="font-mono text-[10px] text-primary">
          {Math.round(item.latencyMs)}ms
        </Text>
      ) : null}
      {item.byteSize != null ? (
        <Text className="font-mono text-[10px] text-muted-foreground">
          {item.byteSize}b
        </Text>
      ) : null}
    </View>
  );
}

function Empty({ label }: { label: string }) {
  return (
    <View className="flex-1 items-center justify-center p-6">
      <Text className="text-center text-[14px] text-muted-foreground">
        {label}
      </Text>
    </View>
  );
}

function fieldMap(entry: BridgethingDiagEntry): Record<string, string> {
  const out: Record<string, string> = {};
  for (const f of entry.fields ?? []) out[f.key] = f.value;
  return out;
}

function yesno(value: boolean): string {
  return value ? 'held' : 'no';
}

function formatTime(ts: number): string {
  const d = new Date(ts);
  return (
    String(d.getHours()).padStart(2, '0') +
    ':' +
    String(d.getMinutes()).padStart(2, '0') +
    ':' +
    String(d.getSeconds()).padStart(2, '0') +
    '.' +
    String(d.getMilliseconds()).padStart(3, '0')
  );
}
