import type {
  BridgethingCompanionDebug,
  BridgethingSessionSnapshot,
} from '@bridgething/session-react-native';
import type { NativeStackScreenProps } from '@react-navigation/native-stack';
import { RefreshCw } from 'lucide-react-native';
import { type ReactNode, useCallback, useEffect, useState } from 'react';
import { ScrollView, Text, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { Press } from '../components/Press';
import { getSession } from '../lib/session';
import type { RootStackParamList } from '../navigation';

type Props = NativeStackScreenProps<RootStackParamList, 'Debug'>;

export function DebugScreen({}: Props) {
  const [companion, setCompanion] = useState<BridgethingCompanionDebug | null>(
    null,
  );
  const [snapshot, setSnapshot] = useState<BridgethingSessionSnapshot | null>(
    null,
  );

  const refresh = useCallback(async () => {
    const session = getSession();
    const [c, s] = await Promise.allSettled([
      session.companionDebug(),
      session.snapshot(),
    ]);
    setCompanion(c.status === 'fulfilled' ? c.value : null);
    setSnapshot(s.status === 'fulfilled' ? s.value : null);
  }, []);

  useEffect(() => {
    refresh();
    const id = setInterval(refresh, 1500);
    return () => clearInterval(id);
  }, [refresh]);

  return (
    <SafeAreaView edges={['bottom']} className="flex-1 bg-background">
      <View className="border-b border-border bg-surface px-5 py-3">
        <View className="flex-row items-center justify-between">
          <Text className="text-[11px] font-bold uppercase tracking-[0.16em] text-muted-foreground">
            session state
          </Text>
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

      <StateDump companion={companion} snapshot={snapshot} />
    </SafeAreaView>
  );
}

function StateDump({
  companion,
  snapshot,
}: {
  companion: BridgethingCompanionDebug | null;
  snapshot: BridgethingSessionSnapshot | null;
}) {
  if (!companion && !snapshot) {
    return <Empty label="no session state (sign in + connect)" />;
  }

  const np = snapshot?.nowPlaying;
  const repeat = np?.playback.repeatMode ?? 'off';

  return (
    <ScrollView contentContainerClassName="px-5 py-4 pb-12">
      {np ? (
        <Section title="now playing">
          <Row label="track id" value={np.track?.id ?? '-'} wrap />
          <Row label="title" value={np.track?.title ?? '-'} />
          <Row label="artist" value={np.track?.artist ?? '-'} />
          <Row label="album" value={np.track?.album ?? '-'} />
          <Row label="duration" value={ms(np.track?.durationMs)} />
          <Row label="position" value={ms(np.playback.positionMs)} />
          <Row
            label="state"
            value={np.playback.playing ? 'playing' : 'paused'}
          />
          <Row label="shuffle" value={yesno(np.playback.shuffle)} />
          <Row label="repeat" value={repeat} />
          <Row label="app" value={np.appName ?? '-'} />
        </Section>
      ) : null}

      {snapshot ? (
        <Section title="connection">
          {snapshot.peers.length === 0 ? (
            <Row label="peers" value="none" />
          ) : (
            snapshot.peers.map(p => (
              <Row
                key={p.id}
                label={p.name || p.id}
                value={
                  p.status === 'connected'
                    ? 'connected'
                    : `link failed${p.linkError ? `: ${p.linkError}` : ''}`
                }
                wrap
              />
            ))
          )}
          <Row label="auth" value={snapshot.authState.kind} />
          <Row
            label="service"
            value={
              snapshot.serviceHealth.kind +
              (snapshot.serviceHealth.retryAfterSeconds != null
                ? ` (${snapshot.serviceHealth.retryAfterSeconds}s)`
                : '')
            }
          />
          <Row label="ANCS" value={snapshot.ancsAuthStatus} />
          {snapshot.provider ? (
            <Row
              label="provider"
              value={`${snapshot.provider.displayName}${snapshot.provider.available ? '' : ' (unavailable)'}`}
            />
          ) : null}
        </Section>
      ) : null}

      {companion ? (
        <Section title="companion">
          <Row
            label="authority · playback"
            value={yesno(companion.authorityPlaybackHeld)}
          />
          <Row
            label="authority · metadata"
            value={yesno(companion.authorityMetadataHeld)}
          />
          <Row
            label="baseline poll"
            value={companion.baselinePollActive ? 'running' : 'idle'}
          />
          <Row
            label="hint fetch"
            value={companion.hintFetchActive ? 'in flight' : 'idle'}
          />
          <Row label="ANCS auth" value={companion.ancsAuthStatus} />
        </Section>
      ) : null}

      {snapshot?.deviceMeta.map(d => (
        <Section key={d.deviceId} title={`device · ${d.deviceId}`}>
          <Row label="model" value={d.meta.modelName || '-'} />
          <Row label="serial" value={d.meta.serialNumber || '-'} wrap />
          <Row label="channel" value={d.meta.channel || '-'} />
          <Row label="daemon" value={d.meta.daemonVersion || '-'} />
          <Row label="image" value={d.meta.imageVersion || '-'} />
          <Row
            label="os"
            value={`${d.meta.osName} ${d.meta.osVersion}`.trim() || '-'}
          />
        </Section>
      ))}

      {snapshot ? (
        <Section title="capabilities">
          <Row label="geo" value={yesno(snapshot.capabilityFlags.geo)} />
          <Row
            label="notifications"
            value={yesno(snapshot.capabilityFlags.notifications)}
          />
          <Row
            label="net · fetch"
            value={yesno(snapshot.capabilityFlags.netFetch)}
          />
          <Row label="net · ws" value={yesno(snapshot.capabilityFlags.netWs)} />
          <Row
            label="audio · tts"
            value={yesno(snapshot.capabilityFlags.audioTts)}
          />
        </Section>
      ) : null}

      {snapshot?.otaPollConfig ? (
        <Section title="ota poll">
          <Row label="channel" value={snapshot.otaPollConfig.channel} />
          <Row
            label="interval"
            value={`${snapshot.otaPollConfig.intervalSeconds}s`}
          />
          <Row
            label="auto-push"
            value={yesno(snapshot.otaPollConfig.autoPush)}
          />
          {snapshot.otaPollConfig.rootUrl ? (
            <Row label="root" value={snapshot.otaPollConfig.rootUrl} wrap />
          ) : null}
        </Section>
      ) : null}

      {snapshot ? (
        <Section title="host">
          <Row
            label="app"
            value={`${snapshot.hostInfo.appName} ${snapshot.hostInfo.appVersion}`}
          />
          <Row
            label="os"
            value={`${snapshot.hostInfo.osName} ${snapshot.hostInfo.osVersion}`.trim()}
          />
          <Row label="lib" value={snapshot.hostInfo.libVersion} />
          <Row
            label="libbridgething"
            value={snapshot.hostInfo.libbridgethingVersion}
          />
          <Row label="adapter" value={snapshot.hostInfo.adapterVersion} />
          <Row label="host id" value={snapshot.hostInfo.hostIdentifier} wrap />
        </Section>
      ) : null}
    </ScrollView>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <View className="mb-5">
      <Text className="mb-1.5 font-mono text-[10px] font-bold uppercase tracking-[0.16em] text-primary">
        {title}
      </Text>
      {children}
    </View>
  );
}

function Row({
  label,
  value,
  wrap,
}: {
  label: string;
  value: string;
  wrap?: boolean;
}) {
  return (
    <View className="flex-row items-start justify-between gap-3 border-b border-border/50 py-2">
      <Text className="text-[13px] text-muted-foreground">{label}</Text>
      <Text
        className="flex-1 text-right font-mono text-[13px] font-semibold text-foreground"
        numberOfLines={wrap ? undefined : 1}
      >
        {value}
      </Text>
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

function ms(value: number | undefined): string {
  if (value == null) return '-';
  const total = Math.round(value / 1000);
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${String(s).padStart(2, '0')}`;
}

function yesno(value: boolean): string {
  return value ? 'held' : 'no';
}
