import {
  type BridgethingActiveWebapp,
  type BridgethingSessionPeer,
  type BridgethingWebappInfo,
  peerDisplayName,
} from '@bridgething/session-react-native';
import type { NativeStackScreenProps } from '@react-navigation/native-stack';
import { useCallback, useEffect, useState } from 'react';
import { Alert, Image, Pressable, ScrollView, Text, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { Button } from '../components/Button';
import { Card } from '../components/Card';
import { Empty, Section } from '../components/Section';
import { getSession, useSessionEvents, useSessionValue } from '../lib/session';
import type { RootStackParamList } from '../navigation';

type Props = NativeStackScreenProps<RootStackParamList, 'Dashboard'>;

/**
 * Stacked device cards: one per connected Car Thing. Each card renders
 * the device's installed apps, active highlight, install CTA, and a
 * nickname-rename affordance.
 */
export function DashboardScreen({ navigation }: Props) {
  const peers = useSessionValue(
    s => s.cachedPeers,
    ['peerConnected', 'peerDisconnected'],
  );
  const provider = useSessionValue(s => s.cachedProvider, ['providerChanged']);

  return (
    <SafeAreaView edges={['bottom']} className="flex-1 bg-background">
      <ScrollView contentContainerClassName="px-5 pb-8 pt-2">
        <Section title="provider">
          <Card>
            <Text className="text-sm font-semibold text-card-foreground">
              {provider?.displayName ?? 'no provider'}
            </Text>
            {provider ? (
              <Text className="mt-0.5 text-xs text-muted-foreground">
                signed in
              </Text>
            ) : null}
          </Card>
        </Section>

        {peers.length === 0 ? (
          <Section title="connection">
            <Card>
              <Text className="text-sm text-foreground">
                no Car Thing connected
              </Text>
              <Text className="mt-1 text-xs text-muted-foreground">
                Plug in your Car Thing or wait for it to wake. The bridge
                auto-pairs once it's in range.
              </Text>
            </Card>
          </Section>
        ) : (
          peers.map(peer => (
            <DeviceCard
              key={peer.id}
              peer={peer}
              onAddApp={() =>
                navigation.navigate('WebappBrowse', { deviceId: peer.id })
              }
              onTapApp={appId =>
                navigation.navigate('WebappDetail', {
                  deviceId: peer.id,
                  id: appId,
                })
              }
            />
          ))
        )}
      </ScrollView>
    </SafeAreaView>
  );
}

function DeviceCard({
  peer,
  onAddApp,
  onTapApp,
}: {
  peer: BridgethingSessionPeer;
  onAddApp: () => void;
  onTapApp: (appId: string) => void;
}) {
  const session = getSession();
  const [webapps, setWebapps] = useState<BridgethingWebappInfo[]>([]);
  const [active, setActive] = useState<BridgethingActiveWebapp | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [refreshError, setRefreshError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setRefreshing(true);
    setRefreshError(null);
    try {
      const [list, current] = await Promise.all([
        session.listWebapps(peer.id),
        session.currentWebapp(peer.id),
      ]);
      setWebapps(list);
      setActive(current);
    } catch (err) {
      setRefreshError(err instanceof Error ? err.message : String(err));
    } finally {
      setRefreshing(false);
    }
  }, [peer.id, session]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useSessionEvents(event => {
    if (event.type === 'webappsChanged' && event.deviceId === peer.id) {
      refresh();
    }
  });

  const switchTo = async (webapp: BridgethingWebappInfo) => {
    try {
      await session.switchWebapp(peer.id, webapp.id);
    } catch (err) {
      Alert.alert(
        'Switch failed',
        err instanceof Error ? err.message : String(err),
      );
    }
  };

  const rename = () => {
    Alert.prompt(
      'Rename device',
      `Local nickname for ${peer.name}. Leave empty to clear.`,
      [
        { text: 'Cancel', style: 'cancel' },
        {
          text: 'Save',
          onPress: (input?: string) => {
            const trimmed = (input ?? '').trim();
            session.setDeviceNickname(peer.id, trimmed === '' ? null : trimmed);
          },
        },
      ],
      'plain-text',
      peer.nickname ?? '',
    );
  };

  return (
    <Section>
      <View className="mb-2 flex-row items-baseline justify-between">
        <View className="flex-1 pr-3">
          <Text className="text-base font-bold text-foreground">
            {peerDisplayName(peer)}
          </Text>
          {peer.nickname ? (
            <Text className="mt-0.5 text-xs text-muted-foreground">
              {peer.name}
            </Text>
          ) : null}
        </View>
        <Pressable onPress={rename} hitSlop={12}>
          <Text className="text-xs font-semibold uppercase tracking-widest text-muted-foreground">
            rename
          </Text>
        </Pressable>
      </View>

      {refreshError ? (
        <Text className="mb-2 text-xs text-destructive">{refreshError}</Text>
      ) : null}

      {webapps.length === 0 ? (
        <Empty>{refreshing ? 'loading…' : 'no apps installed'}</Empty>
      ) : (
        <View className="-mx-1 flex-row flex-wrap">
          {webapps.map(w => {
            const isActive = active?.id === w.id;
            return (
              <Pressable
                key={w.id}
                onLongPress={() => switchTo(w)}
                onPress={() => onTapApp(w.id)}
                className={`m-1 w-[31%] items-center rounded-md p-3 ${isActive ? 'bg-primary' : 'bg-card'}`}
              >
                <WebappIcon deviceId={peer.id} webapp={w} />
                <Text
                  numberOfLines={2}
                  className={`mt-2 text-center text-xs font-semibold ${isActive ? 'text-primary-foreground' : 'text-card-foreground'}`}
                >
                  {w.name}
                </Text>
                {isActive ? (
                  <Text className="mt-0.5 text-[10px] uppercase tracking-wider text-primary-foreground/70">
                    active
                  </Text>
                ) : null}
              </Pressable>
            );
          })}
        </View>
      )}

      <View className="mt-2 flex-row gap-2">
        <Button onPress={onAddApp} variant="secondary">
          add an app
        </Button>
        <Button onPress={refresh} variant="ghost" loading={refreshing}>
          refresh
        </Button>
      </View>
    </Section>
  );
}

function WebappIcon({
  deviceId,
  webapp,
}: {
  deviceId: string;
  webapp: BridgethingWebappInfo;
}) {
  const session = getSession();
  const [dataUri, setDataUri] = useState<string | null>(null);

  useEffect(() => {
    if (!webapp.iconAvailable) return;
    let cancelled = false;
    (async () => {
      try {
        const icon = await session.webappIcon(deviceId, webapp.id);
        if (cancelled || !icon) return;
        setDataUri(`data:${icon.mime ?? 'image/png'};base64,${icon.base64}`);
      } catch {
        // icon load failure is non-fatal
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [deviceId, session, webapp.iconAvailable, webapp.id]);

  if (dataUri) {
    return <Image source={{ uri: dataUri }} className="h-12 w-12 rounded" />;
  }
  return (
    <View className="h-12 w-12 items-center justify-center rounded bg-muted">
      <Text className="text-xl font-bold text-muted-foreground">
        {webapp.name.slice(0, 1).toUpperCase()}
      </Text>
    </View>
  );
}
