import {
  type BridgethingCapabilityFlags,
  type BridgethingDeviceMeta,
  type BridgethingOtaEvent,
  type BridgethingOtaPollConfig,
  peerDisplayName,
} from '@bridgething/session-react-native';
import type { NativeStackScreenProps } from '@react-navigation/native-stack';
import { useCallback, useEffect, useState } from 'react';
import {
  Alert,
  Linking,
  Pressable,
  ScrollView,
  Switch,
  Text,
  View,
} from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { Button } from '../components/Button';
import { Card } from '../components/Card';
import { Empty, Section } from '../components/Section';
import { getSession, useSessionEvents, useSessionValue } from '../lib/session';
import type { RootStackParamList } from '../navigation';

type Props = NativeStackScreenProps<RootStackParamList, 'Settings'>;

const APP_VERSION = '0.1.0';
const REPO_URL = 'https://github.com/JoeyEamigh/bridgething';
const CHANNELS = ['stable', 'dev'] as const;

export function SettingsScreen({ navigation }: Props) {
  const session = getSession();

  const provider = useSessionValue(s => s.cachedProvider, ['providerChanged']);
  const peers = useSessionValue(
    s => s.cachedPeers,
    ['peerConnected', 'peerDisconnected'],
  );

  const [flags, setFlags] = useState<BridgethingCapabilityFlags | null>(null);
  const [pollConfig, setPollConfig] = useState<BridgethingOtaPollConfig | null>(
    null,
  );
  const [metaByDevice, setMetaByDevice] = useState<
    Record<string, BridgethingDeviceMeta>
  >({});
  const [otaEvents, setOtaEvents] = useState<BridgethingOtaEvent[]>([]);
  const [signOutBusy, setSignOutBusy] = useState(false);
  const [pollBusy, setPollBusy] = useState(false);

  const refresh = useCallback(async () => {
    const [f, p] = await Promise.all([
      session.getCapabilityFlags(),
      session.getOtaPollConfig(),
    ]);
    setFlags(f);
    setPollConfig(p);
    // Pull current meta for each connected peer in parallel.
    const metaEntries = await Promise.all(
      peers.map(async peer => {
        const meta = await session.deviceMeta(peer.id);
        return [peer.id, meta] as const;
      }),
    );
    const next: Record<string, BridgethingDeviceMeta> = {};
    for (const [peerId, meta] of metaEntries) {
      if (meta) next[peerId] = meta;
    }
    setMetaByDevice(next);
  }, [peers, session]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useSessionEvents(event => {
    if (event.type === 'otaEvent') {
      setOtaEvents(prev => [event.event, ...prev].slice(0, 25));
    } else if (event.type === 'deviceMetaChanged') {
      setMetaByDevice(prev => ({ ...prev, [event.deviceId]: event.meta }));
    } else if (event.type === 'peerDisconnected') {
      setMetaByDevice(prev => {
        const next = { ...prev };
        delete next[event.peerId];
        return next;
      });
    }
  });

  const writeFlags = async (next: BridgethingCapabilityFlags) => {
    setFlags(next);
    try {
      await session.setCapabilityFlags(next);
    } catch (err) {
      Alert.alert(
        'Save failed',
        err instanceof Error ? err.message : String(err),
      );
    }
  };

  const setChannel = async (channel: string) => {
    const next: BridgethingOtaPollConfig = {
      channel,
      intervalSeconds: pollConfig?.intervalSeconds ?? 21600,
      autoPush: pollConfig?.autoPush ?? true,
      rootUrl: pollConfig?.rootUrl,
    };
    setPollConfig(next);
    await session.setOtaPollConfig(next);
  };

  const toggleAutoPush = async (autoPush: boolean) => {
    if (!pollConfig) {
      // Initialize on first toggle so the user has something to point at.
      const next: BridgethingOtaPollConfig = {
        channel: 'stable',
        intervalSeconds: 21600,
        autoPush,
      };
      setPollConfig(next);
      await session.setOtaPollConfig(next);
      return;
    }
    const next = { ...pollConfig, autoPush };
    setPollConfig(next);
    await session.setOtaPollConfig(next);
  };

  const checkForUpdate = async () => {
    if (!pollConfig) {
      Alert.alert(
        'Pick a channel first',
        'Choose stable or dev to enable update polling.',
      );
      return;
    }
    setPollBusy(true);
    try {
      await session.pollOtaNow();
    } finally {
      setPollBusy(false);
    }
  };

  const signOut = async () => {
    Alert.alert(
      'Sign out?',
      `${provider?.displayName ?? 'this provider'} tokens will be cleared from the keychain.`,
      [
        { text: 'Cancel', style: 'cancel' },
        {
          text: 'Sign out',
          style: 'destructive',
          onPress: async () => {
            setSignOutBusy(true);
            try {
              await session.signOut();
              navigation.reset({ index: 0, routes: [{ name: 'Setup' }] });
            } catch (err) {
              Alert.alert(
                'Sign-out failed',
                err instanceof Error ? err.message : String(err),
              );
            } finally {
              setSignOutBusy(false);
            }
          },
        },
      ],
    );
  };

  return (
    <SafeAreaView edges={['bottom']} className="flex-1 bg-background">
      <ScrollView contentContainerClassName="px-5 pb-8 pt-2">
        <Section title="account">
          <Card>
            <Text className="text-sm font-semibold text-card-foreground">
              {provider?.displayName ?? 'no provider'}
            </Text>
            <Text className="mt-0.5 text-xs text-muted-foreground">
              {provider ? 'signed in' : 'not signed in'}
            </Text>
          </Card>
          {provider ? (
            <View className="mt-2">
              <Button
                onPress={signOut}
                variant="destructive"
                loading={signOutBusy}
              >
                sign out
              </Button>
            </View>
          ) : null}
        </Section>

        <Section title="capabilities">
          <Text className="mb-2 text-xs text-muted-foreground">
            Toggle which gateway-side surfaces the companion announces. The
            daemon re-reads these whenever a Car Thing reconnects.
          </Text>
          {flags ? (
            <Card>
              <FlagRow
                label="location (geo)"
                value={flags.geo}
                onChange={geo => writeFlags({ ...flags, geo })}
              />
              <Divider />
              <FlagRow
                label="iPhone notifications (ANCS)"
                value={flags.notifications}
                onChange={notifications =>
                  writeFlags({ ...flags, notifications })
                }
              />
              <Divider />
              <FlagRow
                label="net.fetch proxy"
                value={flags.netFetch}
                onChange={netFetch => writeFlags({ ...flags, netFetch })}
              />
              <Divider />
              <FlagRow
                label="net.ws proxy"
                value={flags.netWs}
                onChange={netWs => writeFlags({ ...flags, netWs })}
              />
              <Divider />
              <FlagRow
                label="audio TTS earcons"
                value={flags.audioTts}
                onChange={audioTts => writeFlags({ ...flags, audioTts })}
              />
            </Card>
          ) : (
            <Empty>loading…</Empty>
          )}
        </Section>

        <Section title="device updates">
          {peers.length === 0 ? (
            <Empty>connect a Car Thing to see its version</Empty>
          ) : (
            peers.map(peer => {
              const meta = metaByDevice[peer.id];
              return (
                <Card key={peer.id} className="mb-2">
                  <Text className="text-sm font-semibold text-card-foreground">
                    {peerDisplayName(peer)}
                  </Text>
                  {meta ? (
                    <>
                      <Text className="mt-0.5 text-xs text-muted-foreground">
                        {meta.modelName} · {meta.osName}
                      </Text>
                      <Text className="mt-0.5 font-mono text-xs text-muted-foreground">
                        daemon {meta.daemonVersion} · channel {meta.channel}
                      </Text>
                    </>
                  ) : (
                    <Text className="mt-0.5 text-xs italic text-muted-foreground">
                      version pending
                    </Text>
                  )}
                </Card>
              );
            })
          )}

          <Card>
            <Text className="text-xs uppercase tracking-widest text-muted-foreground">
              channel
            </Text>
            <View className="-m-1 mt-1 flex-row flex-wrap">
              {CHANNELS.map(c => {
                const selected = pollConfig?.channel === c;
                return (
                  <Pressable
                    key={c}
                    onPress={() => setChannel(c)}
                    className={`m-1 rounded-md px-3 py-1.5 ${selected ? 'bg-primary' : 'bg-secondary'}`}
                  >
                    <Text
                      className={`text-sm font-semibold ${selected ? 'text-primary-foreground' : 'text-secondary-foreground'}`}
                    >
                      {c}
                    </Text>
                  </Pressable>
                );
              })}
            </View>
            <View className="mt-3">
              <FlagRow
                label="auto-push when an update is detected"
                value={pollConfig?.autoPush ?? false}
                onChange={toggleAutoPush}
              />
            </View>
          </Card>

          <View className="mt-2">
            <Button onPress={checkForUpdate} loading={pollBusy}>
              check for updates now
            </Button>
          </View>

          {otaEvents.length > 0 ? (
            <Card className="mt-3">
              <Text className="mb-1 text-xs uppercase tracking-widest text-muted-foreground">
                recent
              </Text>
              {otaEvents.map((e, i) => (
                <Text
                  key={i}
                  className="font-mono text-[11px] leading-4 text-muted-foreground"
                >
                  {formatOtaEvent(e)}
                </Text>
              ))}
            </Card>
          ) : null}
        </Section>

        <Section title="about">
          <Card>
            <Text className="text-sm font-semibold text-card-foreground">
              bridgething companion
            </Text>
            <Text className="mt-0.5 text-xs text-muted-foreground">
              v{APP_VERSION}
            </Text>
            <Pressable
              onPress={() => Linking.openURL(REPO_URL)}
              className="mt-3"
            >
              <Text className="text-sm font-semibold text-primary">
                {REPO_URL}
              </Text>
            </Pressable>
          </Card>
        </Section>
      </ScrollView>
    </SafeAreaView>
  );
}

function FlagRow({
  label,
  value,
  onChange,
}: {
  label: string;
  value: boolean;
  onChange: (next: boolean) => void;
}) {
  return (
    <View className="flex-row items-center justify-between py-2">
      <Text className="flex-1 pr-3 text-sm text-card-foreground">{label}</Text>
      <Switch value={value} onValueChange={onChange} />
    </View>
  );
}

function Divider() {
  return <View className="h-px bg-border" />;
}

function formatOtaEvent(e: BridgethingOtaEvent): string {
  switch (e.kind) {
    case 'manifestPolled':
      return `manifest @ ${e.updatedAt ?? '?'}`;
    case 'manifestPollFailed':
      return `poll failed: ${e.reason ?? '?'}`;
    case 'channelMismatch':
      return `channel mismatch: device=${e.deviceChannel} cfg=${e.configuredChannel}`;
    case 'updateAvailable':
      return `${e.otaKind} update ${e.fromVersion} → ${e.toVersion}`;
    case 'progress':
      return `${e.otaKind} ${e.phase} ${Math.round(e.percent ?? 0)}%${e.reason ? ` (${e.reason})` : ''}`;
    case 'updated':
      return `${e.otaKind} updated to ${e.toVersion}`;
    case 'failed':
      return `${e.otaKind ?? 'ota'} failed: ${e.reason ?? '?'}`;
    default:
      return JSON.stringify(e);
  }
}
