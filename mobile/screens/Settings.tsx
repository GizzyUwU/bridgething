import {
  type BridgethingCapabilityFlags,
  type BridgethingDeviceMeta,
  type BridgethingOtaPollConfig,
  type BridgethingProviderInfo,
} from '@bridgething/session-react-native';
import type { NativeStackScreenProps } from '@react-navigation/native-stack';
import {
  Activity,
  BatteryCharging,
  Bell,
  Cable,
  ChevronDown,
  ChevronRight,
  Code,
  Globe,
  LifeBuoy,
  LogIn,
  LogOut,
  MapPin,
  MoonStar,
  MoreHorizontal,
  Phone,
  Plus,
  RadioTower,
  RefreshCw,
  Speaker,
  TerminalSquare,
  UserRound,
  Wifi,
} from 'lucide-react-native';
import { useCallback, useEffect, useState } from 'react';
import {
  Alert,
  AppState,
  Linking,
  Platform,
  ScrollView,
  Switch,
  Text,
  ToastAndroid,
  View,
} from 'react-native';
import {
  check,
  request,
  PERMISSIONS,
  RESULTS,
  type PermissionStatus,
} from 'react-native-permissions';
import { SafeAreaView } from 'react-native-safe-area-context';

import { ActionMenu, type MenuAction } from '../components/ActionMenu';
import { Button } from '../components/Button';
import { ListGroup } from '../components/ListGroup';
import { ListRow } from '../components/ListRow';
import { PendingAuth } from '../components/PendingAuth';
import { Pill } from '../components/Pill';
import { Press } from '../components/Press';
import { RenameSheet } from '../components/RenameSheet';
import { ScreenHeader } from '../components/ScreenHeader';
import { SectionEmpty, SectionHeader } from '../components/SectionHeader';
import { ServiceHealthBanner } from '../components/ServiceHealthBanner';
import { Segmented } from '../components/Segmented';
import { type OtaDeviceStatus, useOta } from '../lib/ota';
import {
  connectedPeers,
  forgetKnownDevice,
  getSession,
  type KnownDevice,
  knownDevices,
  peerDisplayName,
  presentPairWithGuidance,
  setDeviceName,
  updateCapabilityFlags,
  updateNickname,
  updateOtaPollConfig,
  useSession,
} from '../lib/session';
import { relativeTime } from '../lib/utils';
import { DEFAULT_OTA_POLL_CONFIG } from '../lib/storage';
import type { RootStackParamList } from '../navigation';

const REPO_URL = 'https://github.com/JoeyEamigh/bridgething';
const CHANNELS = ['stable', 'dev'] as const;

type Props = NativeStackScreenProps<RootStackParamList, 'Settings'>;

export function SettingsScreen({ navigation }: Props) {
  const session = getSession();

  const provider = useSession(s => s.provider);
  const peers = useSession(s => s.peers);
  const authState = useSession(s => s.authState);
  const flags = useSession(s => s.capabilityFlags);
  const pollConfig = useSession(s => s.otaPollConfig);
  const ledger = useSession(s => s.ledger);
  const metaByDevice = useSession(s => s.deviceMeta);
  const host = useSession(s => s.hostInfo);
  const otaByDevice = useOta(s => s.byDevice);

  const livePeers = connectedPeers(peers);
  const known = knownDevices(ledger, peers);
  const selectedChannel = (pollConfig?.channel as 'stable' | 'dev') ?? 'stable';

  const [signOutBusy, setSignOutBusy] = useState(false);
  const [pollBusy, setPollBusy] = useState(false);
  const [providers, setProviders] = useState<BridgethingProviderInfo[]>([]);
  const [signInBusy, setSignInBusy] = useState<string | null>(null);
  const [addDeviceBusy, setAddDeviceBusy] = useState(false);
  const [advancedOpen, setAdvancedOpen] = useState(false);

  const addDevice = async () => {
    if (addDeviceBusy) return;
    setAddDeviceBusy(true);
    try {
      await presentPairWithGuidance();
    } catch (err) {
      Alert.alert(
        'pair failed',
        err instanceof Error ? err.message : String(err),
      );
    } finally {
      setAddDeviceBusy(false);
    }
  };

  const refresh = useCallback(async () => {
    setProviders(await session.availableProviders());
  }, [session]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const writeFlags = async (next: BridgethingCapabilityFlags) => {
    try {
      await updateCapabilityFlags(next);
    } catch (err) {
      Alert.alert(
        'save failed',
        err instanceof Error ? err.message : String(err),
      );
    }
  };

  const writePollConfig = async (
    partial: Partial<BridgethingOtaPollConfig>,
  ) => {
    const next: BridgethingOtaPollConfig = {
      channel: partial.channel ?? pollConfig?.channel ?? selectedChannel,
      intervalSeconds:
        partial.intervalSeconds ??
        pollConfig?.intervalSeconds ??
        DEFAULT_OTA_POLL_CONFIG.intervalSeconds,
      autoPush:
        partial.autoPush ??
        pollConfig?.autoPush ??
        DEFAULT_OTA_POLL_CONFIG.autoPush,
      rootUrl: partial.rootUrl ?? pollConfig?.rootUrl,
    };
    await updateOtaPollConfig(next);
  };

  const checkForUpdate = async () => {
    setPollBusy(true);
    try {
      await session.checkForOtaUpdate(selectedChannel, null);
    } finally {
      setPollBusy(false);
    }
  };

  const installLatest = async (deviceId: string) => {
    try {
      const manifest = await session.fetchOtaManifest(null);
      const latest = manifest.channels.find(
        c => c.slug === selectedChannel,
      )?.latest;
      if (latest) {
        await session.applyOtaUpdate(deviceId, selectedChannel, latest, null);
      }
    } catch (err) {
      Alert.alert(
        'install failed',
        err instanceof Error ? err.message : String(err),
      );
    }
  };

  const signIn = async (id: string) => {
    if (signInBusy) return;
    setSignInBusy(id);
    try {
      await session.setActiveProvider(id);
    } catch {
      // failures surface via authState
    } finally {
      setSignInBusy(null);
    }
  };

  const cancelAuth = () => {
    void session.cancelAuth();
    setSignInBusy(null);
  };

  const signOut = async () => {
    Alert.alert(
      'sign out?',
      `${provider?.displayName ?? 'this provider'} will be signed out on this phone.`,
      [
        { text: 'cancel', style: 'cancel' },
        {
          text: 'sign out',
          style: 'destructive',
          onPress: async () => {
            setSignOutBusy(true);
            try {
              await session.signOut();
            } catch (err) {
              Alert.alert(
                'sign-out failed',
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
      <ScrollView contentContainerClassName="px-5 pb-12 pt-2">
        <ScreenHeader title="settings" />
        <ServiceHealthBanner />

        <View className="mb-7">
          <SectionHeader title="account" />
          <ListGroup>
            <ListRow
              icon={UserRound}
              iconTint="primary"
              title={provider?.displayName ?? 'no provider'}
              subtitle={provider ? 'signed in' : 'not signed in'}
              trailing={
                provider ? (
                  <Pill tone="success" dot={false}>
                    active
                  </Pill>
                ) : null
              }
            />
            {provider ? (
              <ListRow
                icon={LogOut}
                iconTint="destructive"
                title={signOutBusy ? 'signing out…' : 'sign out'}
                destructive
                onPress={signOut}
                loading={signOutBusy}
              />
            ) : (
              providers
                .filter(p => p.available)
                .map(p => (
                  <ListRow
                    key={p.id}
                    icon={LogIn}
                    iconTint="primary"
                    title={`sign in to ${p.displayName}`}
                    chevron
                    onPress={() => signIn(p.id)}
                    loading={signInBusy === p.id}
                  />
                ))
            )}
          </ListGroup>
          {!provider &&
          (authState.kind === 'pending' || authState.kind === 'failed') ? (
            <View className="mt-3">
              <PendingAuth
                state={authState}
                onCancel={authState.kind === 'pending' ? cancelAuth : undefined}
                onRetry={
                  authState.kind === 'failed' && signInBusy === null
                    ? () => {
                        const first = providers.find(p => p.available);
                        if (first) signIn(first.id);
                      }
                    : undefined
                }
              />
            </View>
          ) : null}
        </View>

        <View className="mb-7">
          <SectionHeader title="devices" />
          {known.length === 0 ? (
            <SectionEmpty>connect a Car Thing to see its details</SectionEmpty>
          ) : (
            <ListGroup>
              {known.map(device => (
                <DeviceRow
                  key={device.id}
                  device={device}
                  meta={metaByDevice[device.id]}
                />
              ))}
            </ListGroup>
          )}
          <View className="mt-3">
            <Button
              onPress={addDevice}
              loading={addDeviceBusy}
              icon={Plus}
              variant="tonal"
              size="md"
            >
              add device
            </Button>
          </View>
        </View>

        <View className="mb-7">
          <SectionHeader
            title="updates"
            hint="applies to every connected Car Thing"
          />
          <View className="rounded-2xl border border-border bg-surface p-4">
            <Text className="mb-2 text-[12px] font-bold uppercase tracking-[0.18em] text-muted-foreground">
              channel
            </Text>
            <Segmented
              options={CHANNELS}
              value={selectedChannel}
              onChange={c => writePollConfig({ channel: c })}
            />
            <View className="mt-4 flex-row items-center justify-between">
              <View className="flex-1 pr-3">
                <Text className="text-[14px] font-semibold text-foreground">
                  install updates automatically
                </Text>
                <Text className="mt-0.5 text-[12px] text-muted-foreground">
                  flip off if you want to confirm each one
                </Text>
              </View>
              <Switch
                value={pollConfig?.autoPush ?? DEFAULT_OTA_POLL_CONFIG.autoPush}
                onValueChange={autoPush => writePollConfig({ autoPush })}
              />
            </View>
          </View>

          <View className="mt-3">
            <Button
              onPress={checkForUpdate}
              loading={pollBusy}
              variant="tonal"
              icon={RefreshCw}
            >
              check for updates now
            </Button>
          </View>

          {livePeers.map(peer => (
            <OtaDeviceCard
              key={peer.id}
              name={peerDisplayName(peer, ledger, metaByDevice[peer.id])}
              status={otaByDevice[peer.id]}
              onInstall={() => installLatest(peer.id)}
              onPickVersion={() =>
                navigation.navigate('OtaVersions', {
                  deviceId: peer.id,
                  channel: selectedChannel,
                })
              }
            />
          ))}
        </View>

        <View className="mb-7">
          <Press
            onPress={() => setAdvancedOpen(v => !v)}
            scaleTo={0.99}
            fade={false}
            className="mb-2 flex-row items-center gap-2 px-2 py-1"
          >
            {advancedOpen ? (
              <ChevronDown
                size={16}
                color="hsl(215 14% 50%)"
                strokeWidth={2.4}
              />
            ) : (
              <ChevronRight
                size={16}
                color="hsl(215 14% 50%)"
                strokeWidth={2.4}
              />
            )}
            <Text className="text-[12px] font-bold uppercase tracking-[0.18em] text-muted-foreground">
              advanced
            </Text>
          </Press>
          {advancedOpen ? (
            <View>
              <Text className="mb-2 px-2 text-[12px] leading-[18px] text-muted-foreground">
                capabilities your phone offers to webapps on the Car Thing. most
                users should leave these alone.
              </Text>
              <ListGroup>
                <GeoFlagRow
                  value={flags.geo}
                  onChange={geo => writeFlags({ ...flags, geo })}
                />
                {Platform.OS === 'android' && flags.geo ? (
                  <BackgroundLocationRow />
                ) : null}
                {Platform.OS === 'ios' ? (
                  <FlagRow
                    icon={Bell}
                    title="iPhone notifications"
                    subtitle="forward iPhone notifications to the Car Thing"
                    value={flags.notifications}
                    onChange={notifications =>
                      writeFlags({ ...flags, notifications })
                    }
                  />
                ) : null}
                {Platform.OS === 'android' ? (
                  <NotificationListenerRow
                    value={flags.notifications}
                    onChange={notifications =>
                      writeFlags({ ...flags, notifications })
                    }
                  />
                ) : null}
                {Platform.OS === 'android' ? <DefaultDialerRow /> : null}
                {Platform.OS === 'android' ? <BatteryOptimizationRow /> : null}
                <FlagRow
                  icon={Globe}
                  title="HTTP proxy"
                  subtitle="phone-relayed HTTP for webapps"
                  value={flags.netFetch}
                  onChange={netFetch => writeFlags({ ...flags, netFetch })}
                />
                <FlagRow
                  icon={Wifi}
                  title="WebSocket proxy"
                  subtitle="phone-relayed websockets for webapps"
                  value={flags.netWs}
                  onChange={netWs => writeFlags({ ...flags, netWs })}
                />
                <FlagRow
                  icon={Speaker}
                  title="phone speaker"
                  subtitle="let the Car Thing play sound through this phone"
                  value={flags.audioTts}
                  onChange={audioTts => writeFlags({ ...flags, audioTts })}
                />
              </ListGroup>
            </View>
          ) : null}
        </View>

        <View className="mb-7">
          <SectionHeader title="diagnostics" />
          <ListGroup>
            <ListRow
              icon={Activity}
              iconTint="default"
              title="debug inspector"
              subtitle="now-playing merge, wire frames, companion state"
              chevron
              onPress={() => navigation.navigate('Debug')}
            />
            <ListRow
              icon={TerminalSquare}
              iconTint="default"
              title="live log stream"
              subtitle="device logs over Bluetooth (slows connection while active)"
              chevron
              onPress={() => navigation.navigate('Logs')}
            />
          </ListGroup>
        </View>

        <View className="mb-2">
          <SectionHeader title="about" />
          <ListGroup>
            <ListRow
              icon={LifeBuoy}
              iconTint="primary"
              title={`${host?.appName ?? 'bridgething'} companion`}
              subtitle={
                host
                  ? `v${host.appVersion} · ${host.osName} ${host.osVersion}`
                  : 'loading…'
              }
            />
            {host ? (
              <ListRow
                icon={RadioTower}
                iconTint="default"
                title="protocol"
                subtitle={`lib ${host.libVersion} · wire ${host.libbridgethingVersion}`}
                value={host.adapterVersion}
              />
            ) : null}
            <ListRow
              icon={Code}
              iconTint="default"
              title="source"
              subtitle={REPO_URL.replace('https://', '')}
              chevron
              onPress={() => Linking.openURL(REPO_URL)}
            />
          </ListGroup>
        </View>
      </ScrollView>
    </SafeAreaView>
  );
}

function DeviceRow({
  device,
  meta,
}: {
  device: KnownDevice;
  meta?: BridgethingDeviceMeta;
}) {
  const [renameOpen, setRenameOpen] = useState(false);
  const [deviceNameOpen, setDeviceNameOpen] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const connected = device.peer?.status === 'connected';
  const linkFailed = device.peer?.status === 'linkFailed';
  const title = device.nickname ?? meta?.nickname ?? device.displayName;

  const menuActions: MenuAction[] = [
    { label: 'rename', onPress: () => setRenameOpen(true) },
  ];
  if (connected) {
    menuActions.push({
      label: 'set device name',
      onPress: () => setDeviceNameOpen(true),
    });
  }
  if (!connected && !linkFailed) {
    menuActions.push({
      label: 'forget this device',
      destructive: true,
      onPress: () => forgetKnownDevice(device.id),
    });
  }

  const submitDeviceName = (value: string | null) => {
    void setDeviceName(device.id, value).catch((err: unknown) => {
      Alert.alert(
        'device name not saved',
        err instanceof Error ? err.message : String(err),
      );
    });
  };

  const subtitle = connected
    ? meta
      ? `${meta.modelName} · ${meta.osName}`
      : 'reading device info…'
    : linkFailed
      ? 'attached, but the link did not open'
      : device.lastConnectedAt > 0
        ? `last connected ${relativeTime(device.lastConnectedAt)}`
        : 'not connected';

  return (
    <>
      <ActionMenu
        visible={menuOpen}
        title={title}
        actions={menuActions}
        onClose={() => setMenuOpen(false)}
      />
      <RenameSheet
        visible={renameOpen}
        title="rename your Car Thing"
        message="this nickname only shows up here on your phone."
        initialValue={device.nickname ?? ''}
        placeholder={device.peer?.name ?? device.displayName}
        onSubmit={value => updateNickname(device.id, value)}
        onClose={() => setRenameOpen(false)}
      />
      <RenameSheet
        visible={deviceNameOpen}
        title="name your Car Thing"
        message="this name lives on the device and shows on its screen."
        initialValue={meta?.nickname ?? ''}
        placeholder={meta?.modelName ?? 'Car Thing'}
        onSubmit={submitDeviceName}
        onClose={() => setDeviceNameOpen(false)}
      />
      <ListRow
        icon={Cable}
        iconTint={connected ? 'primary' : 'default'}
        title={title}
        subtitle={subtitle}
        value={
          connected && meta
            ? `${meta.daemonVersion}+${meta.imageVersion}`
            : undefined
        }
        trailing={
          <MoreHorizontal
            size={18}
            color="hsl(215 14% 60%)"
            strokeWidth={2.2}
          />
        }
        onPress={() => setMenuOpen(true)}
      />
    </>
  );
}

function OtaDeviceCard({
  name,
  status,
  onInstall,
  onPickVersion,
}: {
  name: string;
  status?: OtaDeviceStatus;
  onInstall: () => void;
  onPickVersion: () => void;
}) {
  const available = status?.availableTo ?? null;
  const installing = status?.installing ?? false;

  return (
    <View className="mt-3 rounded-2xl border border-border bg-surface p-4">
      <Text className="text-[14px] font-semibold text-foreground">{name}</Text>
      {installing ? (
        <Text className="mt-1 text-[12px] text-muted-foreground">
          {status?.phase ?? 'installing'} · {status?.percent ?? 0}%
        </Text>
      ) : available ? (
        <View className="mt-2">
          <Text className="mb-2 text-[12px] text-muted-foreground">
            update available: {available}
          </Text>
          <Button onPress={onInstall} size="md">
            install update
          </Button>
        </View>
      ) : status?.phase === 'completed' ? (
        <Text className="mt-1 text-[12px] text-muted-foreground">
          rebooting to complete installation...
        </Text>
      ) : null}
      {status?.error ? (
        <Text className="mt-2 text-[12px] text-destructive">
          {status.error}
        </Text>
      ) : null}
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
    </View>
  );
}

function FlagRow({
  icon,
  title,
  subtitle,
  value,
  onChange,
}: {
  icon: import('lucide-react-native').LucideIcon;
  title: string;
  subtitle?: string;
  value: boolean;
  onChange: (next: boolean) => void;
}) {
  return (
    <ListRow
      icon={icon}
      iconTint="default"
      title={title}
      subtitle={subtitle}
      trailing={<Switch value={value} onValueChange={onChange} />}
    />
  );
}

function GeoFlagRow({
  value,
  onChange,
}: {
  value: boolean;
  onChange: (next: boolean) => void;
}) {
  // using the wrong permission constant causes the toggle to silently spring back.
  const permission =
    Platform.OS === 'android'
      ? PERMISSIONS.ANDROID.ACCESS_FINE_LOCATION
      : PERMISSIONS.IOS.LOCATION_WHEN_IN_USE;

  const [status, setStatus] = useState<PermissionStatus | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      const s = await check(permission);
      if (!cancelled) setStatus(s);
    })();
    return () => {
      cancelled = true;
    };
  }, [permission]);

  const denied = status === RESULTS.DENIED || status === RESULTS.BLOCKED;
  const granted = status === RESULTS.GRANTED || status === RESULTS.LIMITED;
  const blocked = status === RESULTS.BLOCKED;
  const subtitle = blocked
    ? 'denied at the system level'
    : 'forward your phone’s location to webapps that ask';

  const handleToggle = async (next: boolean) => {
    if (!next) {
      onChange(false);
      return;
    }
    if (granted) {
      onChange(true);
      return;
    }
    if (blocked) return;
    setBusy(true);
    try {
      const result = await request(permission);
      setStatus(result);
      if (result === RESULTS.GRANTED || result === RESULTS.LIMITED) {
        onChange(true);
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <ListRow
      icon={MapPin}
      iconTint={denied ? 'destructive' : 'default'}
      title="location"
      subtitle={subtitle}
      onPress={blocked ? () => Linking.openSettings() : undefined}
      trailing={
        <Switch
          value={value && granted}
          onValueChange={handleToggle}
          disabled={busy || blocked}
        />
      }
    />
  );
}

function BackgroundLocationRow() {
  const permission = PERMISSIONS.ANDROID.ACCESS_BACKGROUND_LOCATION;
  const [status, setStatus] = useState<PermissionStatus | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    setStatus(await check(permission));
  }, [permission]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // recheck on foreground; user may have changed the permission in system settings
  useEffect(() => {
    const sub = AppState.addEventListener('change', next => {
      if (next === 'active') refresh();
    });
    return () => sub.remove();
  }, [refresh]);

  const granted = status === RESULTS.GRANTED;
  const blocked = status === RESULTS.BLOCKED;
  const subtitle = granted
    ? 'allowed all the time'
    : blocked
      ? 'denied at the system level'
      : 'forward fixes even when the app is in the background';

  const handleToggle = async (next: boolean) => {
    if (!next) {
      // revoking only the bg variant downgrades to while-using; drop fine+coarse for a full revoke
      Alert.alert(
        'restart bridgething?',
        'to revoke location, android needs to kill + restart the app. revoke now?',
        [
          { text: 'later', style: 'cancel' },
          {
            text: 'revoke + restart',
            style: 'destructive',
            onPress: async () => {
              setBusy(true);
              try {
                const session = getSession();
                const scheduled = await session.revokeRuntimePermissions([
                  PERMISSIONS.ANDROID.ACCESS_BACKGROUND_LOCATION,
                  PERMISSIONS.ANDROID.ACCESS_FINE_LOCATION,
                  PERMISSIONS.ANDROID.ACCESS_COARSE_LOCATION,
                ]);
                if (!scheduled) {
                  Alert.alert(
                    'revoke in settings',
                    'this android version needs you to revoke location from system settings. open it?',
                    [
                      { text: 'cancel', style: 'cancel' },
                      {
                        text: 'open settings',
                        onPress: () => Linking.openSettings(),
                      },
                    ],
                  );
                  return;
                }
                ToastAndroid.show(
                  'restarting bridgething…',
                  ToastAndroid.SHORT,
                );
                // give the toast a beat to render before killing the process
                setTimeout(() => {
                  session.killApp().catch(() => {});
                }, 500);
              } finally {
                setBusy(false);
              }
            },
          },
        ],
      );
      return;
    }
    setBusy(true);
    try {
      const result = await request(permission);
      setStatus(result);
      if (result === RESULTS.BLOCKED) {
        if (Platform.OS === 'android') {
          ToastAndroid.show(
            'tap "Allow all the time" in the perms screen',
            ToastAndroid.LONG,
          );
        }
        Alert.alert(
          'background location blocked',
          'android requires "Allow all the time" from the system settings page. open it?',
          [
            { text: 'cancel', style: 'cancel' },
            { text: 'open settings', onPress: () => Linking.openSettings() },
          ],
        );
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <ListRow
      icon={MoonStar}
      iconTint={blocked ? 'destructive' : 'default'}
      title="background location"
      subtitle={subtitle}
      onPress={blocked ? () => Linking.openSettings() : undefined}
      trailing={
        <Switch value={granted} onValueChange={handleToggle} disabled={busy} />
      }
    />
  );
}

function NotificationListenerRow({
  value,
  onChange,
}: {
  value: boolean;
  onChange: (next: boolean) => void;
}) {
  const session = getSession();
  const [granted, setGranted] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setGranted(await session.isNotificationAccessGranted());
    } catch {
      setGranted(false);
    }
  }, [session]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // recheck on foreground; user may have toggled access in system settings
  useEffect(() => {
    const sub = AppState.addEventListener('change', next => {
      if (next === 'active') refresh();
    });
    return () => sub.remove();
  }, [refresh]);

  const subtitle = granted
    ? 'forwarding to the Car Thing'
    : 'tap to open notification access in Settings';

  const openSettings = async (mode: 'grant' | 'revoke') => {
    try {
      await session.requestNotificationAccess();
      if (Platform.OS === 'android') {
        ToastAndroid.show(
          mode === 'grant'
            ? 'tap bridgething and allow access'
            : 'tap bridgething and revoke access',
          ToastAndroid.LONG,
        );
      }
    } catch (err) {
      Alert.alert(
        'failed to open settings',
        err instanceof Error ? err.message : String(err),
      );
    }
  };

  const handleToggle = async (next: boolean) => {
    if (!next) {
      // no programmatic revoke for NotificationListenerService
      Alert.alert(
        'revoke in settings',
        'android only lets you revoke notification access from system settings. open it?',
        [
          { text: 'leave as-is', style: 'cancel' },
          {
            text: 'open settings',
            onPress: () => {
              onChange(false);
              openSettings('revoke');
            },
          },
        ],
      );
      return;
    }
    if (granted) {
      onChange(true);
      return;
    }
    setBusy(true);
    try {
      await openSettings('grant');
      onChange(true);
    } finally {
      setBusy(false);
    }
  };

  return (
    <ListRow
      icon={Bell}
      iconTint="default"
      title="notification access"
      subtitle={subtitle}
      onPress={!granted ? () => openSettings('grant') : undefined}
      trailing={
        <Switch
          value={Boolean(value && granted)}
          onValueChange={handleToggle}
          disabled={busy}
        />
      }
    />
  );
}

function DefaultDialerRow() {
  const session = getSession();
  const [granted, setGranted] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setGranted(await session.isDefaultDialer());
    } catch {
      setGranted(false);
    }
  }, [session]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    const sub = AppState.addEventListener('change', next => {
      if (next === 'active') refresh();
    });
    return () => sub.remove();
  }, [refresh]);

  const subtitle = granted
    ? 'mirroring calls to the Car Thing'
    : 'tap to make bridgething your default phone app';

  const request = async () => {
    setBusy(true);
    try {
      await session.requestDefaultDialer();
      await refresh();
    } catch (err) {
      Alert.alert(
        'failed to request',
        err instanceof Error ? err.message : String(err),
      );
    } finally {
      setBusy(false);
    }
  };

  const handleToggle = async (next: boolean) => {
    if (!next) {
      Alert.alert(
        'change in settings',
        'android only lets you change the default phone app from system settings.',
      );
      return;
    }
    if (!granted) await request();
  };

  return (
    <ListRow
      icon={Phone}
      iconTint="default"
      title="phone calls"
      subtitle={subtitle}
      onPress={!granted ? request : undefined}
      trailing={
        <Switch
          value={Boolean(granted)}
          onValueChange={handleToggle}
          disabled={busy}
        />
      }
    />
  );
}

function BatteryOptimizationRow() {
  const session = getSession();
  const [exempt, setExempt] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setExempt(await session.isIgnoringBatteryOptimizations());
    } catch {
      setExempt(false);
    }
  }, [session]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    const sub = AppState.addEventListener('change', next => {
      if (next === 'active') refresh();
    });
    return () => sub.remove();
  }, [refresh]);

  const subtitle = exempt
    ? 'background connection is protected from doze'
    : 'tap to keep the Car Thing connected in the background';

  const request = async () => {
    setBusy(true);
    try {
      await session.requestIgnoreBatteryOptimizations();
      await refresh();
    } catch (err) {
      Alert.alert(
        'failed to request',
        err instanceof Error ? err.message : String(err),
      );
    } finally {
      setBusy(false);
    }
  };

  const handleToggle = async (next: boolean) => {
    if (!next) {
      Alert.alert(
        'change in settings',
        'android only lets you re-enable battery optimization from system settings.',
      );
      return;
    }
    if (!exempt) await request();
  };

  return (
    <ListRow
      icon={BatteryCharging}
      iconTint="default"
      title="background connection"
      subtitle={subtitle}
      onPress={!exempt ? request : undefined}
      trailing={
        <Switch
          value={Boolean(exempt)}
          onValueChange={handleToggle}
          disabled={busy}
        />
      }
    />
  );
}
