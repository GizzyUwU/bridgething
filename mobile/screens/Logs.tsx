import type { NativeStackScreenProps } from '@react-navigation/native-stack';
import {
  ArrowDown,
  FolderClock,
  Pause,
  Play,
  Share2,
  Trash2,
} from 'lucide-react-native';
import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  Alert,
  FlatList,
  type NativeScrollEvent,
  type NativeSyntheticEvent,
  Share,
  Text,
  TextInput,
  View,
} from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { LogArchiveSheet } from '../components/LogArchiveSheet';
import { Press } from '../components/Press';
import { Segmented } from '../components/Segmented';
import { type DeviceLogLine, useDiagnostics } from '../lib/diagnostics';
import { getSession } from '../lib/session';
import type { RootStackParamList } from '../navigation';

type Props = NativeStackScreenProps<RootStackParamList, 'Logs'>;

/** Threshold filter: picking a level shows it and everything more severe. */
const LEVELS = ['all', 'info', 'warn', 'error'] as const;
type LevelFilter = (typeof LEVELS)[number];

const SEVERITY: Record<string, number> = {
  debug: 0,
  info: 1,
  warn: 2,
  error: 3,
};

/** Past this scroll offset the list is no longer tailing and we stop auto-following. */
const TAIL_SLOP_PX = 24;

export function LogsScreen({}: Props) {
  const entries = useDiagnostics(s => s.deviceLogs);
  const deviceStreaming = useDiagnostics(s => s.deviceLogStreaming);
  const localStreaming = useDiagnostics(s => s.localLogStreaming);
  const setDeviceStreaming = useDiagnostics(s => s.setDeviceLogStreaming);
  const setLocalStreaming = useDiagnostics(s => s.setLocalLogStreaming);
  const clearLogs = useDiagnostics(s => s.clearDeviceLogs);
  const streaming = deviceStreaming || localStreaming;

  const [filter, setFilter] = useState<LevelFilter>('all');
  const [query, setQuery] = useState('');
  const [storedBytes, setStoredBytes] = useState(0);
  const [atTail, setAtTail] = useState(true);
  const [archivesOpen, setArchivesOpen] = useState(false);
  const listRef = useRef<FlatList<DeviceLogLine>>(null);

  /**
   * Newest first, because the list renders inverted: that pins the newest line
   * to the visual bottom for free and keeps position stable as lines arrive,
   * instead of chasing the tail with scrollToEnd on every batch.
   */
  const visible = useMemo(() => {
    const min = filter === 'all' ? -1 : SEVERITY[filter];
    const needle = query.trim().toLowerCase();
    const out: DeviceLogLine[] = [];
    for (let i = entries.length - 1; i >= 0; i--) {
      const e = entries[i];
      if (min >= 0 && (SEVERITY[e.level] ?? 0) < min) continue;
      if (needle && !e.message.toLowerCase().includes(needle)) continue;
      out.push(e);
    }
    return out;
  }, [entries, filter, query]);

  const refreshStored = useCallback(() => {
    getSession()
      .persistedLogSize()
      .then(setStoredBytes)
      .catch(() => setStoredBytes(0));
  }, []);

  useEffect(refreshStored, [refreshStored]);

  const onScroll = useCallback((e: NativeSyntheticEvent<NativeScrollEvent>) => {
    setAtTail(e.nativeEvent.contentOffset.y <= TAIL_SLOP_PX);
  }, []);

  const jumpToTail = useCallback(() => {
    listRef.current?.scrollToOffset({ offset: 0, animated: true });
  }, []);

  /**
   * Prefers the on-disk bundle (full logcat across the last few launches) and
   * falls back to dumping the in-memory buffer as text on platforms that have
   * no persistent store.
   */
  const share = useCallback(async () => {
    try {
      if (await getSession().shareLogs()) return;
    } catch {
      // fall through to the in-memory path
    }
    if (entries.length === 0) {
      Alert.alert('Nothing to share', 'Log buffer is empty.');
      return;
    }
    try {
      await Share.share({ message: entries.map(formatEntry).join('\n') });
    } catch (err) {
      Alert.alert(
        'Share failed',
        err instanceof Error ? err.message : String(err),
      );
    }
  }, [entries]);

  const clearStored = useCallback(() => {
    Alert.alert(
      'Clear stored logs?',
      'Deletes the log files kept on disk from previous app launches, including launches pinned because they contained errors.',
      [
        { text: 'Cancel', style: 'cancel' },
        {
          text: 'Clear',
          style: 'destructive',
          onPress: () => {
            getSession()
              .clearPersistedLogs()
              .catch(() => {})
              .finally(refreshStored);
          },
        },
      ],
    );
  }, [refreshStored]);

  return (
    <SafeAreaView edges={['bottom']} className="flex-1 bg-background">
      <View className="border-b border-border bg-surface px-4 pb-2.5 pt-3">
        <Segmented
          options={LEVELS}
          value={filter}
          onChange={setFilter}
          size="sm"
        />

        <View className="mt-2.5 flex-row items-center gap-2">
          <TextInput
            value={query}
            onChangeText={setQuery}
            placeholder="filter messages"
            placeholderTextColor="hsl(215 16% 55%)"
            autoCapitalize="none"
            autoCorrect={false}
            className="h-8 flex-1 rounded-lg bg-secondary px-2.5 font-mono text-[12px] text-foreground"
          />
          <ToolbarBtn
            icon={deviceStreaming ? Pause : Play}
            label="device"
            active={deviceStreaming}
            onPress={() => setDeviceStreaming(!deviceStreaming)}
          />
          <ToolbarBtn
            icon={localStreaming ? Pause : Play}
            label="phone"
            active={localStreaming}
            onPress={() => setLocalStreaming(!localStreaming)}
          />
          <ToolbarBtn icon={FolderClock} onPress={() => setArchivesOpen(true)} />
          <ToolbarBtn icon={Share2} onPress={share} />
          <ToolbarBtn icon={Trash2} onPress={clearLogs} destructive />
        </View>

        <View className="mt-2 flex-row items-center justify-between">
          <Text className="text-[11px] text-muted-foreground">
            {visible.length === entries.length
              ? `${entries.length} lines`
              : `${visible.length} of ${entries.length} lines`}
            {streaming ? ' · streaming' : ' · stopped'}
          </Text>
          {storedBytes > 0 ? (
            <Press onPress={clearStored} scaleTo={0.96} hitSlop={8}>
              <Text className="text-[11px] text-muted-foreground">
                {formatBytes(storedBytes)} on disk ·{' '}
                <Text className="text-destructive">clear</Text>
              </Text>
            </Press>
          ) : null}
        </View>
      </View>

      {visible.length === 0 ? (
        <View className="flex-1 items-center justify-center p-6">
          <Text className="text-center text-[13px] text-muted-foreground">
            {emptyMessage(entries.length, streaming, query, filter)}
          </Text>
        </View>
      ) : (
        <View className="flex-1">
          <FlatList
            ref={listRef}
            inverted
            data={visible}
            keyExtractor={keyExtractor}
            renderItem={renderRow}
            onScroll={onScroll}
            scrollEventThrottle={64}
            removeClippedSubviews
            initialNumToRender={24}
            maxToRenderPerBatch={24}
            windowSize={9}
            contentContainerClassName="px-3 py-2"
          />
          {atTail ? null : (
            <Press
              onPress={jumpToTail}
              scaleTo={0.92}
              className="absolute bottom-4 self-center flex-row items-center gap-1.5 rounded-full bg-primary px-3 py-1.5"
            >
              <ArrowDown size={12} color="white" strokeWidth={2.6} />
              <Text className="text-[11px] font-bold uppercase tracking-[0.14em] text-white">
                latest
              </Text>
            </Press>
          )}
        </View>
      )}

      <LogArchiveSheet
        visible={archivesOpen}
        onClose={() => setArchivesOpen(false)}
        onChanged={refreshStored}
      />
    </SafeAreaView>
  );
}

function keyExtractor(e: DeviceLogLine): string {
  return e.id;
}

function renderRow({ item }: { item: DeviceLogLine }) {
  return <Row item={item} />;
}

/**
 * Memoized on the entry object, which the store never mutates in place, so a
 * new batch only renders the rows it actually added.
 */
const Row = memo(function Row({ item }: { item: DeviceLogLine }) {
  const { tag, body } = splitTag(item.message);
  return (
    <View className="border-b border-border/40 py-1.5">
      <View className="flex-row items-center gap-2">
        <View className={`rounded px-1.5 py-0.5 ${levelBg(item.level)}`}>
          <Text
            className={`font-mono text-[9px] font-bold uppercase ${levelText(item.level)}`}
          >
            {item.level.slice(0, 4)}
          </Text>
        </View>
        <Text className="font-mono text-[10px] text-muted-foreground">
          {formatTime(item.ts)}
        </Text>
        {tag ? (
          <Text
            numberOfLines={1}
            className="flex-1 font-mono text-[10px] text-muted-foreground/80"
          >
            {tag}
          </Text>
        ) : null}
      </View>
      <Text className="mt-0.5 font-mono text-[12px] leading-[16px] text-foreground">
        {body}
      </Text>
    </View>
  );
});

function ToolbarBtn({
  icon: Icon,
  label,
  onPress,
  destructive,
  active,
}: {
  icon: import('lucide-react-native').LucideIcon;
  label?: string;
  onPress: () => void;
  destructive?: boolean;
  active?: boolean;
}) {
  const color = destructive ? 'hsl(0 72% 50%)' : 'hsl(199 100% 44%)';
  return (
    <Press
      onPress={onPress}
      scaleTo={0.92}
      hitSlop={6}
      className={`h-8 flex-row items-center gap-1 rounded-full px-2 ${
        destructive ? 'bg-destructive-soft' : 'bg-primary-soft'
      } ${active ? 'border border-primary' : ''}`}
    >
      <Icon size={12} color={color} strokeWidth={2.4} />
      {label ? (
        <Text
          className={`text-[10px] font-bold uppercase tracking-[0.1em] ${
            destructive ? 'text-destructive' : 'text-primary'
          }`}
        >
          {label}
        </Text>
      ) : null}
    </Press>
  );
}

/** Native formats lines as `[tag] message`; pull the tag out so it can sit in the meta row. */
function splitTag(message: string): { tag: string | null; body: string } {
  const m = /^\[([^\]]{1,48})\]\s?([\s\S]*)$/.exec(message);
  return m ? { tag: m[1], body: m[2] } : { tag: null, body: message };
}

function emptyMessage(
  total: number,
  streaming: boolean,
  query: string,
  filter: LevelFilter,
): string {
  if (total === 0) {
    return streaming
      ? 'streaming; no log lines yet'
      : 'press device or phone to stream logs';
  }
  if (query.trim()) return `no lines match "${query.trim()}"`;
  return `no lines at ${filter} or above`;
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

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatEntry(e: DeviceLogLine): string {
  return `[${formatTime(e.ts)}] ${e.level.toUpperCase().padEnd(5)} ${e.message}`;
}
