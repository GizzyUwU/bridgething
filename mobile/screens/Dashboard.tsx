import {
  type BridgethingActiveWebapp,
  type BridgethingSessionPeer,
  type BridgethingWebappInfo,
} from '@bridgething/session-react-native';
import type { NativeStackScreenProps } from '@react-navigation/native-stack';
import { AppWindow, Cable, Pencil, Plus, RefreshCw } from 'lucide-react-native';
import { useCallback, useEffect, useState } from 'react';
import { Image, ScrollView, Text, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { HeroPulse } from '../components/HeroPulse';
import { Press } from '../components/Press';
import { RenameSheet } from '../components/RenameSheet';
import { ScreenHeader } from '../components/ScreenHeader';
import { SectionHeader } from '../components/SectionHeader';
import { StatusStrip } from '../components/StatusStrip';
import {
  getSession,
  peerDisplayName,
  updateNickname,
  useSession,
} from '../lib/session';
import type { RootStackParamList } from '../navigation';

type Props = NativeStackScreenProps<RootStackParamList, 'Dashboard'>;

export function DashboardScreen({ navigation }: Props) {
  const peers = useSession(s => s.peers);
  const provider = useSession(s => s.provider);
  const ancsStatus = useSession(s => s.ancsAuthStatus);
  const nicknames = useSession(s => s.nicknames);

  const paired = ancsStatus !== 'unknown';
  const signedIn = provider != null;

  const status = describeStatus({
    signedIn,
    paired,
    hasPeer: peers.length > 0,
  });

  return (
    <SafeAreaView edges={['bottom']} className="flex-1 bg-background">
      <ScrollView contentContainerClassName="px-5 pb-12 pt-2">
        <ScreenHeader
          title="your bridge"
          subtitle="bridgething is running on this phone."
        />

        <View className="mb-5">
          <StatusStrip
            tone={status.tone}
            title={status.title}
            subtitle={status.subtitle}
            onPress={
              status.action
                ? () => navigation.navigate('Setup', { startAt: status.action })
                : undefined
            }
          />
        </View>

        {peers.length === 0 ? <NoDeviceHero paired={paired} /> : null}
        {peers.map(peer => (
          <DeviceSection
            key={peer.id}
            peer={peer}
            nickname={nicknames[peer.id]}
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
        ))}
      </ScrollView>
    </SafeAreaView>
  );
}

type StatusDescriptor = {
  tone: 'good' | 'info' | 'warn';
  title: string;
  subtitle?: string;
  action?: 'signIn' | 'pair';
};

function describeStatus({
  signedIn,
  paired,
  hasPeer,
}: {
  signedIn: boolean;
  paired: boolean;
  hasPeer: boolean;
}): StatusDescriptor {
  if (!signedIn) {
    return {
      tone: 'warn',
      title: 'sign in to your music',
      subtitle: 'tap to finish setup',
      action: 'signIn',
    };
  }
  if (!paired) {
    return {
      tone: 'warn',
      title: 'pair your Car Thing',
      subtitle: 'tap to pair when your Car Thing is on',
      action: 'pair',
    };
  }
  if (!hasPeer) {
    return {
      tone: 'info',
      title: 'Car Thing not in range',
      subtitle: 'we’ll connect as soon as it powers on',
    };
  }
  return {
    tone: 'good',
    title: 'everything connected',
    subtitle: 'open your Car Thing to use it',
  };
}

function NoDeviceHero({ paired }: { paired: boolean }) {
  return (
    <View className="mb-8 items-center px-2 py-10">
      <HeroPulse tint="primary" />
      <Text
        className="mt-8 text-center text-foreground"
        style={{
          fontFamily: 'Outfit-Medium',
          fontSize: 22,
          lineHeight: 26,
          letterSpacing: -0.5,
        }}
      >
        {paired ? 'Car Thing not in range' : 'no Car Thing yet'}
      </Text>
      <Text className="mt-2 text-center text-[14px] leading-[20px] text-muted-foreground">
        {paired
          ? 'plug it in or wake it up. the bridge auto-connects when it’s within Bluetooth range.'
          : 'pair your Car Thing from the status strip above to get going.'}
      </Text>
    </View>
  );
}

function DeviceSection({
  peer,
  nickname,
  onAddApp,
  onTapApp,
}: {
  peer: BridgethingSessionPeer;
  nickname: string | undefined;
  onAddApp: () => void;
  onTapApp: (appId: string) => void;
}) {
  const session = getSession();
  const nicknames = useSession(s => s.nicknames);
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

  // Refresh on webapps-changed events scoped to this device.
  useEffect(() => {
    const off = session.subscribe(event => {
      if (event.type === 'webappsChanged' && event.deviceId === peer.id) {
        refresh();
      }
    });
    return off;
  }, [peer.id, refresh, session]);

  const [renameOpen, setRenameOpen] = useState(false);

  return (
    <View className="mb-8">
      <RenameSheet
        visible={renameOpen}
        title="rename your Car Thing"
        message="this nickname only shows up here on your phone."
        initialValue={nickname ?? ''}
        placeholder={peer.name}
        onSubmit={value => updateNickname(peer.id, value)}
        onClose={() => setRenameOpen(false)}
      />
      <View
        className="mb-4 flex-row items-center gap-3 rounded-2xl border border-border bg-surface p-4"
        style={{
          shadowColor: '#000',
          shadowOpacity: 0.06,
          shadowRadius: 14,
          shadowOffset: { width: 0, height: 6 },
        }}
      >
        <View className="h-11 w-11 items-center justify-center rounded-2xl bg-primary-soft">
          <Cable size={20} color="hsl(199 100% 44%)" strokeWidth={2.2} />
        </View>
        <View className="flex-1">
          <Text
            className="text-[17px] font-extrabold text-foreground"
            numberOfLines={1}
            style={{ letterSpacing: -0.3 }}
          >
            {peerDisplayName(peer, nicknames)}
          </Text>
          <View className="mt-1 flex-row items-center gap-1.5">
            <View className="h-1.5 w-1.5 rounded-full bg-success" />
            <Text
              className="text-[12px] text-muted-foreground"
              numberOfLines={1}
            >
              connected{nickname ? ` · ${peer.name}` : ''}
            </Text>
          </View>
        </View>
        <Press
          onPress={() => setRenameOpen(true)}
          className="h-9 w-9 items-center justify-center rounded-full bg-secondary"
          scaleTo={0.92}
        >
          <Pencil size={14} color="hsl(215 14% 38%)" strokeWidth={2.4} />
        </Press>
      </View>

      <SectionHeader
        title="installed apps"
        action={refreshing ? '' : 'refresh'}
        onActionPress={refresh}
      />
      {refreshError ? (
        <View className="mb-3 rounded-2xl border border-destructive/30 bg-destructive-soft px-4 py-3">
          <Text className="text-[12px] text-destructive">{refreshError}</Text>
        </View>
      ) : null}

      <View className="-mx-1 flex-row flex-wrap">
        {webapps.map(w => {
          const isActive = active?.id === w.id;
          return (
            <AppTile
              key={w.id}
              webapp={w}
              deviceId={peer.id}
              active={isActive}
              onTap={() => onTapApp(w.id)}
            />
          );
        })}
        <AddTile onPress={onAddApp} />
        {refreshing && webapps.length === 0 ? (
          <View className="m-1 w-[31%] items-center justify-center rounded-2xl border border-border bg-surface px-3 py-6">
            <RefreshCw size={18} color="hsl(215 14% 50%)" strokeWidth={2.2} />
            <Text className="mt-2 text-[11px] text-muted-foreground">
              loading
            </Text>
          </View>
        ) : null}
      </View>
    </View>
  );
}

function AppTile({
  webapp,
  deviceId,
  active,
  onTap,
}: {
  webapp: BridgethingWebappInfo;
  deviceId: string;
  active: boolean;
  onTap: () => void;
}) {
  return (
    <Press onPress={onTap} className="m-1 w-[31%]" scaleTo={0.94}>
      <View
        className={`items-center rounded-2xl border px-3 py-4 ${
          active ? 'border-primary bg-primary-soft' : 'border-border bg-surface'
        }`}
        style={
          active
            ? {
                shadowColor: 'hsl(199 100% 44%)',
                shadowOpacity: 0.18,
                shadowRadius: 12,
                shadowOffset: { width: 0, height: 6 },
                elevation: 2,
              }
            : {
                shadowColor: '#000',
                shadowOpacity: 0.04,
                shadowRadius: 8,
                shadowOffset: { width: 0, height: 3 },
              }
        }
      >
        <WebappIcon deviceId={deviceId} webapp={webapp} />
        <Text
          numberOfLines={2}
          className={`mt-2.5 text-center text-[12px] font-semibold leading-[15px] ${
            active ? 'text-primary' : 'text-foreground'
          }`}
        >
          {webapp.name}
        </Text>
        {active ? (
          <Text className="mt-1 text-[9px] font-bold uppercase tracking-[0.2em] text-primary">
            active
          </Text>
        ) : null}
      </View>
    </Press>
  );
}

function AddTile({ onPress }: { onPress: () => void }) {
  return (
    <Press onPress={onPress} className="m-1 w-[31%]" scaleTo={0.94}>
      <View
        className="items-center justify-center rounded-2xl border-2 border-dashed border-border bg-transparent px-3 py-4"
        style={{ minHeight: 110 }}
      >
        <View className="h-12 w-12 items-center justify-center rounded-2xl bg-secondary">
          <Plus size={22} color="hsl(215 14% 38%)" strokeWidth={2.4} />
        </View>
        <Text className="mt-2.5 text-center text-[12px] font-semibold leading-[15px] text-muted-foreground">
          add app
        </Text>
      </View>
    </Press>
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
  const [uri, setUri] = useState<string | null>(null);

  useEffect(() => {
    if (!webapp.iconAvailable) return;
    let cancelled = false;
    (async () => {
      try {
        const icon = await session.webappIcon(deviceId, webapp.id);
        if (cancelled || !icon) return;
        setUri(icon.fileUri);
      } catch {
        // icon load failure is non-fatal
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [deviceId, session, webapp.iconAvailable, webapp.id]);

  if (uri) {
    return (
      <Image
        source={{ uri }}
        className="h-12 w-12 rounded-xl"
        resizeMode="cover"
      />
    );
  }
  return (
    <View className="h-12 w-12 items-center justify-center rounded-xl bg-secondary">
      {webapp.name ? (
        <Text className="text-[18px] font-extrabold text-foreground">
          {webapp.name.slice(0, 1).toUpperCase()}
        </Text>
      ) : (
        <AppWindow size={20} color="hsl(215 14% 38%)" strokeWidth={2.2} />
      )}
    </View>
  );
}
