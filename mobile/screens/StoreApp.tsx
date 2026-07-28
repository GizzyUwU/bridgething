import { sortNewestFirst, type AppVersion } from '@bridgething/catalog';
import type { NativeStackScreenProps } from '@react-navigation/native-stack';
import {
  Bell,
  Cable,
  Globe,
  LayoutGrid,
  type LucideIcon,
  MapPin,
  Mic,
  Shield,
  Speaker,
  Wifi,
} from 'lucide-react-native';
import { useState } from 'react';
import { Alert, Linking, Text, View } from 'react-native';

import { Button } from '../components/Button';
import { CatalogIcon } from '../components/CatalogIcon';
import { ListGroup } from '../components/ListGroup';
import { ListRow } from '../components/ListRow';
import { Pill } from '../components/Pill';
import { Press } from '../components/Press';
import { ScrollScreen } from '../components/ScrollScreen';
import { SectionEmpty, SectionHeader } from '../components/SectionHeader';
import { installApp, useSourceListings } from '../lib/catalog';
import { useOtaProgress } from '../lib/ota';
import type { RootStackParamList } from '../navigation';

type Props = NativeStackScreenProps<RootStackParamList, 'StoreApp'>;

export function StoreAppScreen({ route }: Props) {
  const { deviceId, appId, sourceUrl } = route.params;
  const listings = useSourceListings(sourceUrl, deviceId);
  const listing = listings.find(l => l.app.id === appId) ?? null;

  const progress = useOtaProgress(deviceId);
  const installingThis =
    progress && !progress.run.outcome && progress.run.webappId === appId
      ? progress
      : null;

  const [failed, setFailed] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [showAllVersions, setShowAllVersions] = useState(false);

  if (!listing) {
    return (
      <ScrollScreen>
        <SectionEmpty>this app is no longer listed by that source</SectionEmpty>
      </ScrollScreen>
    );
  }

  const { app, newestCompatible, installedVersion, updateAvailable } = listing;
  const incompatible = !newestCompatible;
  const canAct = !incompatible && (!installedVersion || updateAvailable);

  const install = async () => {
    if (!deviceId) {
      Alert.alert(
        'No Car Thing connected',
        `connect one to install ${app.name}. browsing works without it.`,
      );
      return;
    }
    setFailed(null);
    setStarting(true);
    try {
      await installApp(deviceId, listing);
    } catch (err) {
      setFailed(err instanceof Error ? err.message : String(err));
    } finally {
      setStarting(false);
    }
  };

  const ordered = sortNewestFirst(app.versions);
  const visibleVersions = showAllVersions ? ordered : ordered.slice(0, 1);

  return (
    <ScrollScreen contentContainerStyle={{ paddingTop: 12 }}>
      <View className="mb-6 flex-row items-center gap-4">
        <CatalogIcon url={app.icon} name={app.name} size={64} />
        <View className="flex-1">
          <Text
            className="text-[20px] font-extrabold leading-[24px] text-foreground"
            numberOfLines={2}
            style={{ letterSpacing: -0.4 }}
          >
            {app.name}
          </Text>
          <Text className="mt-0.5 text-[13px] text-muted-foreground">
            {app.author}
          </Text>
          <View className="mt-2 flex-row flex-wrap gap-1.5">
            {installedVersion ? (
              <Pill tone="primary">{`installed v${installedVersion}`}</Pill>
            ) : null}
            {newestCompatible?.role === 'launcher' ? (
              <Pill tone="neutral" dot={false}>
                home screen
              </Pill>
            ) : null}
            {newestCompatible?.provides_overlay ? (
              <Pill tone="neutral" dot={false}>
                overlay
              </Pill>
            ) : null}
          </View>
        </View>
      </View>

      {installingThis ? (
        <View className="mb-6 rounded-2xl border border-border bg-surface p-4">
          <View className="flex-row items-baseline justify-between">
            <Text className="text-[13px] font-semibold text-foreground">
              {installingThis.stepLabel ?? 'installing'}
            </Text>
            <Text className="text-[12px] text-muted-foreground">
              {installingThis.percent}%
            </Text>
          </View>
          <View className="mt-2 h-2 overflow-hidden rounded-full bg-muted">
            <View
              className="h-full rounded-full bg-primary"
              style={{ width: `${installingThis.percent}%` }}
            />
          </View>
        </View>
      ) : (
        <View className="mb-6">
          <Button
            onPress={install}
            disabled={!canAct || starting}
            loading={starting}
          >
            {incompatible
              ? 'needs a newer firmware'
              : updateAvailable
                ? `update to v${newestCompatible?.version}`
                : installedVersion
                  ? 'installed'
                  : `install v${newestCompatible?.version}`}
          </Button>
        </View>
      )}

      {failed ? (
        <View className="mb-6 rounded-2xl border border-destructive/30 bg-destructive-soft px-4 py-3">
          <Text className="text-[12px] text-destructive">{failed}</Text>
        </View>
      ) : null}

      <Text className="mb-6 px-1 text-[14px] leading-[20px] text-foreground">
        {app.description}
      </Text>

      {newestCompatible ? (
        <View className="mb-8">
          <SectionHeader title="what this app can do" />
          {newestCompatible.permissions.length === 0 ? (
            <SectionEmpty>nothing beyond drawing on the screen</SectionEmpty>
          ) : (
            <ListGroup>
              {newestCompatible.permissions.map(p => {
                const meta = humanizePermission(p);
                return (
                  <ListRow
                    key={p}
                    icon={meta.icon}
                    iconTint="default"
                    title={meta.title}
                    subtitle={meta.subtitle}
                  />
                );
              })}
            </ListGroup>
          )}
        </View>
      ) : null}

      <View className="mb-8">
        <SectionHeader title="versions" />
        <ListGroup>
          {visibleVersions.map(v => (
            <VersionRow
              key={v.version}
              version={v}
              installed={v.version === installedVersion}
            />
          ))}
        </ListGroup>
        {app.versions.length > 1 ? (
          <Press
            onPress={() => setShowAllVersions(v => !v)}
            scaleTo={0.99}
            fade={false}
            className="mt-2 py-1"
          >
            <Text className="text-[13px] font-semibold text-primary">
              {showAllVersions
                ? 'show fewer'
                : `show all ${app.versions.length} versions`}
            </Text>
          </Press>
        ) : null}
      </View>

      <View>
        <SectionHeader title="where this came from" />
        <ListGroup>
          <ListRow icon={LayoutGrid} title="source" subtitle={sourceUrl} />
          {app.homepage ? (
            <ListRow
              icon={Globe}
              title="homepage"
              subtitle={app.homepage}
              onPress={() => void Linking.openURL(app.homepage as string)}
            />
          ) : null}
          {app.source ? (
            <ListRow
              icon={Globe}
              title="source code"
              subtitle={app.source}
              onPress={() => void Linking.openURL(app.source as string)}
            />
          ) : null}
        </ListGroup>
        <Text className="mt-2 px-1 text-[11.5px] leading-[16px] text-muted-foreground">
          apps are not reviewed. a listing means a source published it, never
          that anyone checked what it does.
        </Text>
      </View>
    </ScrollScreen>
  );
}

function VersionRow({
  version,
  installed,
}: {
  version: AppVersion;
  installed: boolean;
}) {
  return (
    <View className="px-4 py-3">
      <View className="flex-row items-center gap-2">
        <Text className="font-mono text-[13px] font-semibold text-foreground">
          v{version.version}
        </Text>
        {installed ? <Pill tone="primary">installed</Pill> : null}
        <Text className="ml-auto text-[11px] text-muted-foreground">
          {new Date(version.released_at).toLocaleDateString()}
        </Text>
      </View>
      {version.changelog ? (
        <Text className="mt-1 text-[12.5px] leading-[18px] text-muted-foreground">
          {version.changelog}
        </Text>
      ) : null}
      <Text className="mt-1 text-[11px] text-muted-foreground">
        needs libbridgething {version.min_libbridgething_version} ·{' '}
        {formatBytes(version.download.size)}
      </Text>
    </View>
  );
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${Math.round(n / 1024)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

function humanizePermission(perm: string): {
  icon: LucideIcon;
  title: string;
  subtitle?: string;
} {
  switch (perm) {
    case 'net.fetch':
      return {
        icon: Globe,
        title: 'use the internet',
        subtitle: 'data fetched via your phone',
      };
    case 'net.ws':
      return {
        icon: Wifi,
        title: 'real-time data',
        subtitle: 'websockets via your phone',
      };
    case 'net.proxy':
      return {
        icon: Cable,
        title: 'tunnel TCP traffic',
        subtitle: 'general TCP via your phone',
      };
    case 'geo':
      return {
        icon: MapPin,
        title: 'see your location',
        subtitle: 'forwarded from your phone',
      };
    case 'notifications':
      return { icon: Bell, title: 'show phone notifications' };
    case 'audio.tts':
    case 'audio':
      return {
        icon: Speaker,
        title: 'play sound',
        subtitle: 'plays through your phone',
      };
    case 'mic':
      return { icon: Mic, title: 'use the Car Thing microphone' };
    default:
      return { icon: Shield, title: perm };
  }
}
