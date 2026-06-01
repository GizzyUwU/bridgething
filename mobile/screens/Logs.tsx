import type { NativeStackScreenProps } from '@react-navigation/native-stack';
import { Pause, Play, Share2, Trash2 } from 'lucide-react-native';
import { useMemo, useState } from 'react';
import {
  Alert,
  FlatList,
  type ListRenderItemInfo,
  Share,
  Text,
  View,
} from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { Press } from '../components/Press';
import { Segmented } from '../components/Segmented';
import { type DeviceLogLine, useDiagnostics } from '../lib/diagnostics';
import type { RootStackParamList } from '../navigation';

type Props = NativeStackScreenProps<RootStackParamList, 'Logs'>;

const LEVELS = ['all', 'debug', 'info', 'warn', 'error'] as const;
type LevelFilter = (typeof LEVELS)[number];

export function LogsScreen({}: Props) {
  const entries = useDiagnostics(s => s.deviceLogs);
  const streaming = useDiagnostics(s => s.logStreaming);
  const setStreaming = useDiagnostics(s => s.setLogStreaming);
  const clearLogs = useDiagnostics(s => s.clearDeviceLogs);
  const [filter, setFilter] = useState<LevelFilter>('all');

  const filtered = useMemo(() => {
    if (filter === 'all') return entries;
    return entries.filter(e => e.level === filter);
  }, [entries, filter]);

  const share = async () => {
    if (entries.length === 0) {
      Alert.alert('Nothing to share', 'Log buffer is empty.');
      return;
    }
    const text = entries.map(formatEntry).join('\n');
    try {
      await Share.share({ message: text });
    } catch (err) {
      Alert.alert(
        'Share failed',
        err instanceof Error ? err.message : String(err),
      );
    }
  };

  return (
    <SafeAreaView edges={['bottom']} className="flex-1 bg-background">
      <View className="border-b border-border bg-surface px-5 py-3">
        <View className="flex-row items-center gap-2">
          <Segmented
            options={LEVELS}
            value={filter}
            onChange={setFilter}
            size="sm"
          />
        </View>
        <View className="mt-3 flex-row items-center justify-between">
          <Text className="text-[12px] text-muted-foreground">
            {filtered.length} / {entries.length} entries
            {streaming ? ' · streaming' : ' · stopped'}
          </Text>
          <View className="flex-row gap-1.5">
            <ToolbarBtn
              icon={streaming ? Pause : Play}
              label={streaming ? 'stop' : 'start'}
              onPress={() => setStreaming(!streaming)}
            />
            <ToolbarBtn icon={Share2} label="share" onPress={share} />
            <ToolbarBtn
              icon={Trash2}
              label="clear"
              onPress={clearLogs}
              destructive
            />
          </View>
        </View>
      </View>
      {filtered.length === 0 ? (
        <View className="flex-1 items-center justify-center p-6">
          <Text className="text-center text-[14px] text-muted-foreground">
            {entries.length === 0
              ? streaming
                ? 'streaming; no log lines yet'
                : 'press start to stream device logs'
              : 'no entries match this filter'}
          </Text>
        </View>
      ) : (
        <FlatList
          data={filtered}
          keyExtractor={e => String(e.id)}
          renderItem={renderRow}
          contentContainerClassName="px-3 py-2"
        />
      )}
    </SafeAreaView>
  );
}

function ToolbarBtn({
  icon: Icon,
  label,
  onPress,
  destructive,
}: {
  icon: import('lucide-react-native').LucideIcon;
  label: string;
  onPress: () => void;
  destructive?: boolean;
}) {
  const color = destructive ? 'hsl(0 72% 50%)' : 'hsl(199 100% 44%)';
  return (
    <Press
      onPress={onPress}
      scaleTo={0.92}
      className={`flex-row items-center gap-1.5 rounded-full px-2.5 py-1 ${
        destructive ? 'bg-destructive-soft' : 'bg-primary-soft'
      }`}
    >
      <Icon size={12} color={color} strokeWidth={2.4} />
      <Text
        className={`text-[11px] font-bold uppercase tracking-[0.14em] ${
          destructive ? 'text-destructive' : 'text-primary'
        }`}
      >
        {label}
      </Text>
    </Press>
  );
}

function renderRow({ item }: ListRenderItemInfo<DeviceLogLine>) {
  return (
    <View className="border-b border-border/50 py-1.5">
      <View className="flex-row items-center gap-2">
        <View className={`rounded-md px-1.5 py-0.5 ${levelBg(item.level)}`}>
          <Text
            className={`font-mono text-[10px] font-bold uppercase ${levelText(item.level)}`}
          >
            {item.level.padEnd(5).slice(0, 5).trim()}
          </Text>
        </View>
        <Text className="font-mono text-[10px] text-muted-foreground">
          {formatTime(item.ts)}
        </Text>
      </View>
      <Text className="mt-1 font-mono text-[12px] leading-[16px] text-foreground">
        {item.message}
      </Text>
    </View>
  );
}

function levelBg(level: string): string {
  switch (level) {
    case 'error':
      return 'bg-destructive-soft';
    case 'warn':
      return 'bg-warning/15';
    case 'info':
      return 'bg-primary-soft';
    default:
      return 'bg-secondary';
  }
}

function levelText(level: string): string {
  switch (level) {
    case 'error':
      return 'text-destructive';
    case 'warn':
      return 'text-warning';
    case 'info':
      return 'text-primary';
    default:
      return 'text-muted-foreground';
  }
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

function formatEntry(e: DeviceLogLine): string {
  return `[${formatTime(e.ts)}] ${e.level.toUpperCase().padEnd(5)} ${e.message}`;
}
