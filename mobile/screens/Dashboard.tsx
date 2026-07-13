import {
  type BridgethingSessionPeer,
  type BridgethingWebappInfo,
} from '@bridgething/session-react-native';
import type { NativeStackScreenProps } from '@react-navigation/native-stack';
import {
  Cable,
  Pencil,
  Plus,
  RefreshCw,
  TriangleAlert,
} from 'lucide-react-native';
import { useState } from 'react';
import { Alert, Text, View } from 'react-native';

import { HeroPulse } from '../components/HeroPulse';
import { OtaCard, otaHasActivity } from '../components/OtaCard';
import { Press } from '../components/Press';
import { RenameSheet } from '../components/RenameSheet';
import { ScreenHeader } from '../components/ScreenHeader';
import { ScrollScreen } from '../components/ScrollScreen';
import { SectionHeader } from '../components/SectionHeader';
import { ServiceHealthBanner } from '../components/ServiceHealthBanner';
import { StatusStrip } from '../components/StatusStrip';
import { WebappIcon } from '../components/WebappIcon';
import { Button } from '../components/Button';
import {
  alertPairOutcome,
  connectedPeers,
  getSession,
  peerDisplayName,
  runPairFlow,
  setDeviceName,
  updateNickname,
  useSession,
} from '../lib/session';
import { installLatestOta, useOta } from '../lib/ota';
import { refreshWebapps, useWebapps } from '../lib/webapps';
import type { RootStackParamList } from '../navigation';

type Props = NativeStackScreenProps<RootStackParamList, 'Dashboard'>;

export function DashboardScreen({ navigation }: Props) {
  const peers = useSession(s => s.peers);
  const provider = useSession(s => s.provider);
  const ledger = useSession(s => s.ledger);

  const live = connectedPeers(peers);
  const linkFailed = peers.filter(p => p.status === 'linkFailed');
  const hasPeer = live.length > 0;
  const signedIn = provider != null;

  const status = describeStatus({ signedIn, hasPeer });
  const [pairBusy, setPairBusy] = useState(false);

  const runStatusAction = async (action: 'signIn' | 'pair') => {
    if (action === 'signIn') {
      navigation.navigate('Settings');
      return;
    }
    if (pairBusy) return;
    setPairBusy(true);
    try {
      alertPairOutcome(await runPairFlow());
    } finally {
      setPairBusy(false);
    }
  };

  const action = status.action;

  return (
    <ScrollScreen>
      <ScreenHeader
        title="your bridge"
        subtitle="bridgething is running on this phone."
      />
      <ServiceHealthBanner />

      <View className="mb-5">
        <StatusStrip
          tone={status.tone}
          title={status.title}
          subtitle={
            pairBusy && action === 'pair' ? 'pairing…' : status.subtitle
          }
          onPress={action ? () => void runStatusAction(action) : undefined}
        />
      </View>

      {peers.length === 0 ? <NoDeviceHero /> : null}
      {linkFailed.map(peer => (
        <LinkFailedCard key={peer.id} peer={peer} />
      ))}
      {live.map(peer => (
        <DeviceSection
          key={peer.id}
          peer={peer}
          nickname={ledger[peer.id]?.nickname ?? undefined}
          onAddApp={() =>
            navigation.navigate('WebappBrowse', { deviceId: peer.id })
          }
          onTapApp={appId =>
            navigation.navigate('WebappDetail', {
              deviceId: peer.id,
              id: appId,
            })
          }
          onPickOtaVersion={channel =>
            navigation.navigate('OtaVersions', { deviceId: peer.id, channel })
          }
        />
      ))}
    </ScrollScreen>
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
  hasPeer,
}: {
  signedIn: boolean;
  hasPeer: boolean;
}): StatusDescriptor {
  if (!signedIn) {
    return {
      tone: 'warn',
      title: 'sign in to your music',
      subtitle: 'tap to sign in',
      action: 'signIn',
    };
  }
  if (!hasPeer) {
    return {
      tone: 'warn',
      title: 'connect your Car Thing',
      subtitle: 'tap to pair when your Car Thing is on',
      action: 'pair',
    };
  }
  return {
    tone: 'good',
    title: 'everything connected',
    subtitle: 'open your Car Thing to use it',
  };
}

function NoDeviceHero() {
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
        no Car Thing connected
      </Text>
      <Text className="mt-2 text-center text-[14px] leading-[20px] text-muted-foreground">
        the bridge auto-connects when it’s within Bluetooth range
      </Text>
    </View>
  );
}

function LinkFailedCard({ peer }: { peer: BridgethingSessionPeer }) {
  const ledger = useSession(s => s.ledger);
  const [busy, setBusy] = useState(false);

  const reconnect = async () => {
    setBusy(true);
    try {
      await getSession().reconnectPeer(peer.id);
    } finally {
      setBusy(false);
    }
  };

  return (
    <View className="mb-8 rounded-2xl border border-border bg-surface p-4">
      <View className="flex-row items-center gap-3">
        <View className="h-11 w-11 items-center justify-center rounded-2xl bg-secondary">
          <TriangleAlert size={20} color="hsl(38 92% 50%)" strokeWidth={2.2} />
        </View>
        <View className="flex-1">
          <Text
            className="text-[17px] font-extrabold text-foreground"
            numberOfLines={1}
            style={{ letterSpacing: -0.3 }}
          >
            {peerDisplayName(peer, ledger)}
          </Text>
          <Text className="mt-1 text-[12px] text-muted-foreground">
            attached, but the link did not open
          </Text>
        </View>
      </View>
      {peer.linkError ? (
        <Text className="mt-3 text-[12px] text-muted-foreground">
          {peer.linkError}
        </Text>
      ) : null}
      <View className="mt-3">
        <Button onPress={reconnect} loading={busy} variant="tonal" size="md">
          try reconnect
        </Button>
      </View>
    </View>
  );
}

function DeviceSection({
  peer,
  nickname,
  onAddApp,
  onTapApp,
  onPickOtaVersion,
}: {
  peer: BridgethingSessionPeer;
  nickname: string | undefined;
  onAddApp: () => void;
  onTapApp: (appId: string) => void;
  onPickOtaVersion: (channel: string) => void;
}) {
  const ledger = useSession(s => s.ledger);
  const meta = useSession(s => s.deviceMeta[peer.id]);
  const channel = meta?.channel || 'stable';
  const ota = useOta(s => s.byDevice[peer.id]);
  const {
    list: webapps,
    active,
    loading: refreshing,
    error: refreshError,
  } = useWebapps(peer.id);

  const [renameOpen, setRenameOpen] = useState(false);

  return (
    <View className="mb-8">
      <RenameSheet
        visible={renameOpen}
        title="rename your Car Thing"
        message="this renames the device and shows on its screen."
        initialValue={meta?.nickname ?? nickname ?? ''}
        placeholder={peer.name}
        onSubmit={value => {
          void setDeviceName(peer.id, value).catch((err: unknown) => {
            Alert.alert(
              'rename failed',
              err instanceof Error ? err.message : String(err),
            );
          });
          updateNickname(peer.id, null);
        }}
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
            {peerDisplayName(peer, ledger, meta)}
          </Text>
          <View className="mt-1 flex-row items-center gap-1.5">
            <View className="h-1.5 w-1.5 rounded-full bg-success" />
            <Text
              className="text-[12px] text-muted-foreground"
              numberOfLines={1}
            >
              connected
              {peerDisplayName(peer, ledger, meta) !== peer.name
                ? ` · ${peer.name}`
                : ''}
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

      {otaHasActivity(ota) ? (
        <View className="mb-4 -mt-1">
          <OtaCard
            name="software update"
            status={ota}
            onInstall={() => {
              void installLatestOta(peer.id, channel).catch((err: unknown) => {
                Alert.alert(
                  'install failed',
                  err instanceof Error ? err.message : String(err),
                );
              });
            }}
            onPickVersion={() => onPickOtaVersion(channel)}
          />
        </View>
      ) : null}

      <SectionHeader
        title="installed apps"
        action={refreshing ? '' : 'refresh'}
        onActionPress={() => refreshWebapps(peer.id)}
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
        <WebappIcon
          deviceId={deviceId}
          id={webapp.id}
          iconHash={webapp.iconHash}
          name={webapp.name}
          size={48}
          fallbackTextClass="text-[18px] font-extrabold text-foreground"
        />
        <Text
          numberOfLines={2}
          className={`mt-2.5 text-center text-[12px] font-semibold leading-[15px] ${
            active ? 'text-primary' : 'text-foreground'
          }`}
        >
          {webapp.name}
        </Text>
        <Text
          numberOfLines={1}
          className="mt-1 text-[9px] font-bold uppercase tracking-[0.2em] text-primary"
          style={{ opacity: active ? 1 : 0 }}
        >
          active
        </Text>
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
        <Text
          numberOfLines={1}
          className="mt-1 text-[9px] font-bold uppercase tracking-[0.2em] text-primary"
          style={{ opacity: 0 }}
        >
          active
        </Text>
      </View>
    </Press>
  );
}
