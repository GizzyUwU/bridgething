import type {
  BridgethingOtaManifest,
  BridgethingOtaRelease,
} from '@bridgething/session-react-native';
import type { NativeStackScreenProps } from '@react-navigation/native-stack';
import { useCallback, useEffect, useState } from 'react';
import { ActivityIndicator, Alert, ScrollView, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { ListGroup } from '../components/ListGroup';
import { ListRow } from '../components/ListRow';
import { Pill } from '../components/Pill';
import { SectionEmpty, SectionHeader } from '../components/SectionHeader';
import { Segmented } from '../components/Segmented';
import { isRunning, useOtaRun } from '../lib/ota';
import { getSession, useSession } from '../lib/session';
import type { RootStackParamList } from '../navigation';

type Props = NativeStackScreenProps<RootStackParamList, 'OtaVersions'>;

export function OtaVersionsScreen({ route, navigation }: Props) {
  const { deviceId, channel: initialChannel } = route.params;
  const meta = useSession(s => s.deviceMeta[deviceId]);
  const installing = isRunning(useOtaRun(deviceId));

  const [manifest, setManifest] = useState<BridgethingOtaManifest | null>(null);
  const [channel, setChannel] = useState(initialChannel);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      setManifest(await getSession().fetchOtaManifest(null));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setManifest(null);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const channels = manifest?.channels.map(c => c.slug) ?? [];
  const selected = manifest?.channels.find(c => c.slug === channel);
  const releases = manifest ? (selected?.releases ?? []) : null;
  const latest = selected?.latest ?? null;

  const current = meta
    ? `${meta.daemonVersion}+image.${meta.imageVersion}`
    : null;
  const deviceChannel = meta?.channel;

  const install = (release: BridgethingOtaRelease) => {
    const crossChannel =
      deviceChannel && deviceChannel !== channel
        ? ` this moves the device from the ${deviceChannel} channel to ${channel}.`
        : '';
    Alert.alert(
      `install ${release.version}?`,
      `pushes daemon ${release.daemonVersion} and image ${release.imageVersion} to this Car Thing.${crossChannel}`,
      [
        { text: 'cancel', style: 'cancel' },
        {
          text: 'install',
          onPress: () => {
            getSession()
              .applyOtaUpdate(deviceId, channel, release.version, null)
              .catch(() => {});
            navigation.goBack();
          },
        },
      ],
    );
  };

  return (
    <SafeAreaView edges={['bottom']} className="flex-1 bg-background">
      <ScrollView contentContainerClassName="px-5 pb-12 pt-2">
        <SectionHeader
          title={`${channel} channel`}
          hint={current ? `installed: ${current}` : undefined}
        />
        {channels.length > 1 ? (
          <View className="mb-3">
            <Segmented
              options={channels}
              value={channel}
              onChange={setChannel}
            />
          </View>
        ) : null}
        {releases == null && !error ? (
          <View className="items-center py-10">
            <ActivityIndicator />
          </View>
        ) : error ? (
          <SectionEmpty>{error}</SectionEmpty>
        ) : !selected ? (
          <SectionEmpty>{`channel '${channel}' is not in the manifest`}</SectionEmpty>
        ) : releases == null || releases.length === 0 ? (
          <SectionEmpty>no releases on this channel</SectionEmpty>
        ) : (
          <ListGroup>
            {releases.map(r => {
              const isCurrent = current === r.version;
              const tappable = !r.yanked && !isCurrent && !installing;
              return (
                <ListRow
                  key={r.version}
                  title={r.version}
                  subtitle={`daemon ${r.daemonVersion} · image ${r.imageVersion}`}
                  disabled={!tappable}
                  onPress={tappable ? () => install(r) : undefined}
                  trailing={
                    r.yanked ? (
                      <Pill tone="destructive" dot={false}>
                        yanked
                      </Pill>
                    ) : isCurrent ? (
                      <Pill tone="success" dot={false}>
                        installed
                      </Pill>
                    ) : latest === r.version ? (
                      <Pill tone="primary" dot={false}>
                        latest
                      </Pill>
                    ) : r.deprecated ? (
                      <Pill tone="warning" dot={false}>
                        old
                      </Pill>
                    ) : undefined
                  }
                />
              );
            })}
          </ListGroup>
        )}
      </ScrollView>
    </SafeAreaView>
  );
}
