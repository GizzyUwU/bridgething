import {
  type BridgethingCapabilityFlags,
  type BridgethingDeviceMeta,
  type BridgethingHostInfo,
  type BridgethingOtaPollConfig,
  type BridgethingProviderInfo,
} from '@bridgething/session-react-native';
import type { NativeStackScreenProps } from '@react-navigation/native-stack';
import {
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

import { Button } from '../components/Button';
import { ListGroup } from '../components/ListGroup';
import { ListRow } from '../components/ListRow';
import { PendingAuth } from '../components/PendingAuth';
import { Pill } from '../components/Pill';
import { Press } from '../components/Press';
import { ScreenHeader } from '../components/ScreenHeader';
import { SectionEmpty, SectionHeader } from '../components/SectionHeader';
import { Segmented } from '../components/Segmented';
import {
  getSession,
  peerDisplayName,
  updateCapabilityFlags,
  updateOtaPollConfig,
  useSession,
} from '../lib/session';
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
  const nicknames = useSession(s => s.nicknames);

  const [metaByDevice, setMetaByDevice] = useState<
    Record<string, BridgethingDeviceMeta>
  >({});
  const [host, setHost] = useState<BridgethingHostInfo | null>(null);
  const [signOutBusy, setSignOutBusy] = useState(false);
  const [pollBusy, setPollBusy] = useState(false);
  const [providers, setProviders] = useState<BridgethingProviderInfo[]>([]);
  const [signInBusy, setSignInBusy] = useState<string | null>(null);
  const [advancedOpen, setAdvancedOpen] = useState(false);

  const refresh = useCallback(async () => {
    const [h, providerList] = await Promise.all([
      session.hostInfo(),
      session.availableProviders(),
    ]);
    setHost(h);
    setProviders(providerList);
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

  // Live-update meta as the daemon reannounces it; drop entries on
  // disconnect so the rows reflect actual session state.
  useEffect(() => {
    return session.subscribe(event => {
      if (event.type === 'deviceMetaChanged') {
        setMetaByDevice(prev => ({ ...prev, [event.deviceId]: event.meta }));
      } else if (event.type === 'peerDisconnected') {
        setMetaByDevice(prev => {
          const next = { ...prev };
          delete next[event.peerId];
          return next;
        });
      }
    });
  }, [session]);

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

  const setChannel = async (channel: 'stable' | 'dev') => {
    const next: BridgethingOtaPollConfig = {
      channel,
      intervalSeconds:
        pollConfig?.intervalSeconds ?? DEFAULT_OTA_POLL_CONFIG.intervalSeconds,
      autoPush: pollConfig?.autoPush ?? DEFAULT_OTA_POLL_CONFIG.autoPush,
      rootUrl: pollConfig?.rootUrl,
    };
    await updateOtaPollConfig(next);
  };

  const toggleAutoPush = async (autoPush: boolean) => {
    const next: BridgethingOtaPollConfig = {
      channel: pollConfig?.channel ?? DEFAULT_OTA_POLL_CONFIG.channel,
      intervalSeconds:
        pollConfig?.intervalSeconds ?? DEFAULT_OTA_POLL_CONFIG.intervalSeconds,
      autoPush,
      rootUrl: pollConfig?.rootUrl,
    };
    await updateOtaPollConfig(next);
  };

  const checkForUpdate = async () => {
    if (!pollConfig) {
      await updateOtaPollConfig({ ...DEFAULT_OTA_POLL_CONFIG });
    }
    setPollBusy(true);
    try {
      await session.pollOtaNow();
    } finally {
      setPollBusy(false);
    }
  };

  const signIn = async (id: string) => {
    if (signInBusy) return;
    setSignInBusy(id);
    try {
      await session.setActiveProvider(id);
    } catch {
      // setActiveProvider routes failures via authStateChanged
    } finally {
      setSignInBusy(null);
    }
  };

  const cancelAuth = async () => {
    await session.cancelAuth();
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
          {peers.length === 0 ? (
            <SectionEmpty>connect a Car Thing to see its details</SectionEmpty>
          ) : (
            <ListGroup>
              {peers.map(peer => {
                const meta = metaByDevice[peer.id];
                return (
                  <ListRow
                    key={peer.id}
                    icon={Cable}
                    iconTint="primary"
                    title={peerDisplayName(peer, nicknames)}
                    subtitle={
                      meta
                        ? `${meta.modelName} · ${meta.osName}`
                        : 'reading device info…'
                    }
                    value={meta ? `daemon ${meta.daemonVersion}` : undefined}
                  />
                );
              })}
            </ListGroup>
          )}
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
              value={(pollConfig?.channel as 'stable' | 'dev') ?? 'stable'}
              onChange={c => setChannel(c)}
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
                value={pollConfig?.autoPush ?? false}
                onValueChange={toggleAutoPush}
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
              icon={TerminalSquare}
              iconTint="default"
              title="live log stream"
              subtitle="for debugging — pulls real cost while open"
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

/**
 * Geo flag row that gates the toggle on OS-level location permission.
 * Flipping ON when permission is undetermined triggers the system
 * prompt; if denied or restricted, the row stays off and offers to
 * open Settings so the user can flip it themselves.
 */
function GeoFlagRow({
  value,
  onChange,
}: {
  value: boolean;
  onChange: (next: boolean) => void;
}) {
  // Pick the right OS-level permission constant for the running
  // platform. Without this check the row tries to query an iOS
  // permission on android and the toggle silently springs back.
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
    ? 'denied at the system level — tap to open Settings'
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

/**
 * Background-location toggle. Android 10+ requires a separate runtime
 * grant on top of foreground location; from API 30 on the request can't
 * pop a dialog and instead kicks the user to system Settings ("Allow
 * all the time"). The toggle reflects the OS-level state, not a user
 * preference - the OS owns the truth.
 */
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

  // Foreground transitions re-check OS state (user may have flipped
  // the perm from system settings without our knowledge).
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
      ? 'denied at the system level — tap to open Settings'
      : 'forward fixes even when the app is in the background';

  const handleToggle = async (next: boolean) => {
    if (!next) {
      // Revoking only the bg variant just downgrades to "while using"
      // - drop fine+coarse together for a real revoke. Confirm first
      // because the OS only applies the change on next process kill,
      // so we have to restart ourselves to make it stick.
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
                      { text: 'open settings', onPress: () => Linking.openSettings() },
                    ],
                  );
                  return;
                }
                ToastAndroid.show('restarting bridgething…', ToastAndroid.SHORT);
                // Give the toast a beat to render, then kill ourselves.
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
        <Switch
          value={granted}
          onValueChange={handleToggle}
          disabled={busy}
        />
      }
    />
  );
}

/**
 * NotificationListenerService access. The Android equivalent of iOS's
 * ANCS pair flow - the user has to toggle our app on under "Device & app
 * notifications" themselves; there is no programmatic grant. We poll
 * the enabled state every time the screen mounts (and after a Settings
 * trip) so the visual toggle matches the OS truth.
 */
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

  // Re-check whenever the app comes back to the foreground (the user
  // most likely just toggled the switch in the system settings page).
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
      // No programmatic revoke for NotificationListenerService - the
      // user has to flip our app off in system settings.
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
