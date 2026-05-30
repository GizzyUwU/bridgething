import type { BridgethingDiagEntry } from '@bridgething/session-react-native';
import { useMemo, useRef, useState } from 'react';
import { Animated, PanResponder, ScrollView, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { type DeviceLogLine, useDiagnostics } from '../lib/diagnostics';
import { Press } from './Press';
import { Segmented } from './Segmented';

const FILTERS = ['all', 'frames', 'logs', 'state'] as const;
type Filter = (typeof FILTERS)[number];

type FeedClass = 'frame' | 'log' | 'state';
type FeedLine = {
  key: string;
  ts: number;
  cls: FeedClass;
  glyph: string;
  glyphColor: string;
  tag: string;
  text: string;
};

const MAX_RENDERED = 300;

export function DebugOverlay() {
  if (!__DEV__) return null;
  return <Overlay />;
}

function Overlay() {
  const insets = useSafeAreaInsets();
  const entries = useDiagnostics(s => s.entries);
  const deviceLogs = useDiagnostics(s => s.deviceLogs);
  const logStreaming = useDiagnostics(s => s.logStreaming);

  const [expanded, setExpanded] = useState(false);
  const [filter, setFilter] = useState<Filter>('all');
  const [dockTop, setDockTop] = useState(false);

  const lines = useMemo(() => {
    const out: FeedLine[] = [];
    for (const e of entries) out.push(entryToLine(e));
    if (logStreaming) for (const l of deviceLogs) out.push(deviceLogToLine(l));
    out.sort((a, b) => b.ts - a.ts);
    return out;
  }, [entries, deviceLogs, logStreaming]);

  const filtered = useMemo(
    () =>
      filter === 'all' ? lines : lines.filter(l => l.cls === classFor(filter)),
    [lines, filter],
  );

  const latest = filtered[0] ?? null;

  const pan = useRef(new Animated.ValueXY({ x: 0, y: 0 })).current;
  const responder = useRef(
    PanResponder.create({
      onMoveShouldSetPanResponder: (_e, g) =>
        Math.abs(g.dx) > 4 || Math.abs(g.dy) > 4,
      onPanResponderGrant: () => {
        pan.extractOffset();
      },
      onPanResponderMove: Animated.event([null, { dx: pan.x, dy: pan.y }], {
        useNativeDriver: false,
      }),
      onPanResponderRelease: () => {
        pan.flattenOffset();
      },
    }),
  ).current;

  if (expanded) {
    return (
      <View
        pointerEvents="box-none"
        className="absolute inset-0"
        style={{ zIndex: 9999 }}
      >
        <View
          className="absolute left-0 right-0 overflow-hidden border-border/60 bg-[rgba(14,16,21,0.96)]"
          style={
            dockTop
              ? {
                  top: 0,
                  paddingTop: insets.top,
                  height: '52%',
                  borderBottomWidth: 1,
                }
              : {
                  bottom: 0,
                  paddingBottom: insets.bottom,
                  height: '52%',
                  borderTopWidth: 1,
                }
          }
        >
          <View className="flex-row items-center gap-2 px-3 py-2">
            <Segmented
              options={FILTERS}
              value={filter}
              onChange={setFilter}
              size="sm"
            />
            <View className="flex-1" />
            <PillButton
              label={dockTop ? 'dock ↓' : 'dock ↑'}
              onPress={() => setDockTop(v => !v)}
            />
            <PillButton label="close" onPress={() => setExpanded(false)} />
          </View>
          <ScrollView
            className="flex-1"
            contentContainerClassName="px-3 pb-3"
            showsVerticalScrollIndicator
          >
            {filtered.length === 0 ? (
              <Text className="px-1 py-4 font-mono text-[11px] text-[rgba(255,255,255,0.4)]">
                no activity
              </Text>
            ) : (
              filtered
                .slice(0, MAX_RENDERED)
                .map(line => <FeedRow key={line.key} line={line} />)
            )}
          </ScrollView>
        </View>
      </View>
    );
  }

  return (
    <View
      pointerEvents="box-none"
      className="absolute inset-0"
      style={{ zIndex: 9999 }}
    >
      <Animated.View
        {...responder.panHandlers}
        className="absolute"
        style={{
          left: 12,
          right: 12,
          bottom: insets.bottom + 12,
          transform: pan.getTranslateTransform(),
        }}
      >
        <Press
          onPress={() => setExpanded(true)}
          scaleTo={0.98}
          fade={false}
          className="flex-row items-center gap-2 rounded-full bg-[rgba(14,16,21,0.92)] px-3 py-2"
        >
          {latest ? (
            <>
              <Text
                className="font-mono text-[12px]"
                style={{ color: latest.glyphColor }}
              >
                {latest.glyph}
              </Text>
              <Text
                className="font-mono text-[11px] font-bold uppercase text-[rgba(255,255,255,0.55)]"
                numberOfLines={1}
              >
                {latest.tag}
              </Text>
              <Text
                className="flex-1 font-mono text-[11px] text-[rgba(255,255,255,0.9)]"
                numberOfLines={1}
              >
                {latest.text}
              </Text>
            </>
          ) : (
            <Text className="flex-1 font-mono text-[11px] text-[rgba(255,255,255,0.5)]">
              bridgething · waiting for activity
            </Text>
          )}
        </Press>
      </Animated.View>
    </View>
  );
}

function FeedRow({ line }: { line: FeedLine }) {
  return (
    <View className="flex-row items-baseline gap-2 border-b border-[rgba(255,255,255,0.06)] py-1">
      <Text
        className="font-mono text-[11px]"
        style={{ color: line.glyphColor }}
      >
        {line.glyph}
      </Text>
      <Text className="font-mono text-[9px] text-[rgba(255,255,255,0.35)]">
        {formatTime(line.ts)}
      </Text>
      <Text
        className="font-mono text-[10px] font-bold uppercase text-[rgba(255,255,255,0.5)]"
        numberOfLines={1}
        style={{ maxWidth: 96 }}
      >
        {line.tag}
      </Text>
      <Text className="flex-1 font-mono text-[11px] text-[rgba(255,255,255,0.9)]">
        {line.text}
      </Text>
    </View>
  );
}

function PillButton({
  label,
  onPress,
}: {
  label: string;
  onPress: () => void;
}) {
  return (
    <Press
      onPress={onPress}
      scaleTo={0.92}
      fade={false}
      className="rounded-full bg-[rgba(255,255,255,0.1)] px-2.5 py-1"
    >
      <Text className="font-mono text-[10px] font-bold uppercase text-[rgba(255,255,255,0.8)]">
        {label}
      </Text>
    </Press>
  );
}

function classFor(filter: Exclude<Filter, 'all'>): FeedClass {
  return filter === 'frames' ? 'frame' : filter === 'logs' ? 'log' : 'state';
}

const FRAME_OUT = 'hsl(199 100% 60%)';
const FRAME_IN = 'hsl(150 55% 55%)';
const LOG_COLOR = 'hsl(215 16% 62%)';
const STATE_COLOR = 'hsl(265 75% 70%)';

function entryToLine(e: BridgethingDiagEntry): FeedLine {
  if (e.kind === 'frame') {
    const out = e.direction === 'outbound';
    const parts = [e.frameKind ?? ''];
    if (e.latencyMs != null) parts.push(`${Math.round(e.latencyMs)}ms`);
    if (e.byteSize != null) parts.push(`${e.byteSize}b`);
    return {
      key: `e${e.seq}`,
      ts: e.ts,
      cls: 'frame',
      glyph: out ? '↑' : '↓',
      glyphColor: out ? FRAME_OUT : FRAME_IN,
      tag: e.surface ?? e.frameKind ?? 'frame',
      text: parts.filter(Boolean).join(' '),
    };
  }
  if (e.kind === 'log') {
    return {
      key: `e${e.seq}`,
      ts: e.ts,
      cls: 'log',
      glyph: '·',
      glyphColor: LOG_COLOR,
      tag: e.level ?? e.target ?? 'log',
      text: e.message ?? '',
    };
  }
  return {
    key: `e${e.seq}`,
    ts: e.ts,
    cls: 'state',
    glyph: '~',
    glyphColor: STATE_COLOR,
    tag: e.category ?? 'state',
    text: breadcrumbText(e),
  };
}

function breadcrumbText(e: BridgethingDiagEntry): string {
  const fields: Record<string, string> = {};
  for (const f of e.fields ?? []) fields[f.key] = f.value;
  const extras = [fields.reason, fields.track].filter(Boolean).join(' · ');
  return extras ? `${e.detail ?? ''} — ${extras}` : (e.detail ?? '');
}

function deviceLogToLine(l: DeviceLogLine): FeedLine {
  return {
    key: `d${l.id}`,
    ts: l.ts,
    cls: 'log',
    glyph: '›',
    glyphColor: LOG_COLOR,
    tag: l.level,
    text: l.message,
  };
}

function formatTime(ts: number): string {
  const d = new Date(ts);
  return (
    String(d.getHours()).padStart(2, '0') +
    ':' +
    String(d.getMinutes()).padStart(2, '0') +
    ':' +
    String(d.getSeconds()).padStart(2, '0')
  );
}
