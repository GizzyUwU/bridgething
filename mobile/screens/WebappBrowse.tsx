import {
  type BridgethingWebappInfo,
  peerDisplayName,
} from '@bridgething/session-react-native';
import type { NativeStackScreenProps } from '@react-navigation/native-stack';
import { useCallback, useEffect, useState } from 'react';
import {
  Alert,
  Pressable,
  ScrollView,
  Text,
  TextInput,
  View,
} from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { Button } from '../components/Button';
import { Card } from '../components/Card';
import { Empty, Section } from '../components/Section';
import { getSession, useSessionEvents, useSessionValue } from '../lib/session';
import type { RootStackParamList } from '../navigation';

type Props = NativeStackScreenProps<RootStackParamList, 'WebappBrowse'>;

/**
 * v1 install path is "paste a URL". We download the .zip companion-side and
 * push the bytes through the gateway to the route's `deviceId`. The list
 * below shows what's currently installed on that device so the user can
 * uninstall directly here.
 */
export function WebappBrowseScreen({ navigation, route }: Props) {
  const session = getSession();
  const deviceId = route.params.deviceId;
  const [url, setUrl] = useState('');
  const [installing, setInstalling] = useState(false);
  const [installed, setInstalled] = useState<BridgethingWebappInfo[]>([]);
  const [busyUninstall, setBusyUninstall] = useState<string | null>(null);

  const peer = useSessionValue(
    s => s.cachedPeers.find(p => p.id === deviceId) ?? null,
    ['peerConnected', 'peerDisconnected'],
  );

  const refresh = useCallback(async () => {
    try {
      const list = await session.listWebapps(deviceId);
      setInstalled(list);
    } catch {
      // Surface inline as empty; the dashboard surfaces the error path.
    }
  }, [deviceId, session]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useSessionEvents(event => {
    if (event.type === 'webappsChanged' && event.deviceId === deviceId) {
      refresh();
    }
  });

  const install = async () => {
    const trimmed = url.trim();
    if (!trimmed) return;
    setInstalling(true);
    try {
      const info = await session.installWebappFromUrl(deviceId, trimmed);
      setUrl('');
      Alert.alert('Installed', `${info.name} ${info.version}`);
    } catch (err) {
      Alert.alert(
        'Install failed',
        err instanceof Error ? err.message : String(err),
      );
    } finally {
      setInstalling(false);
    }
  };

  const uninstall = (webapp: BridgethingWebappInfo) => {
    if (webapp.source === 'builtin') {
      Alert.alert(
        'Built-in webapp',
        `${webapp.name} ships with the daemon and cannot be uninstalled.`,
      );
      return;
    }
    Alert.alert('Uninstall app?', `${webapp.name} ${webapp.version}`, [
      { text: 'Cancel', style: 'cancel' },
      {
        text: 'Uninstall',
        style: 'destructive',
        onPress: async () => {
          setBusyUninstall(webapp.id);
          try {
            await session.uninstallWebapp(deviceId, webapp.id);
          } catch (err) {
            Alert.alert(
              'Uninstall failed',
              err instanceof Error ? err.message : String(err),
            );
          } finally {
            setBusyUninstall(null);
          }
        },
      },
    ]);
  };

  return (
    <SafeAreaView edges={['bottom']} className="flex-1 bg-background">
      <ScrollView contentContainerClassName="px-5 pb-8 pt-2">
        <Section title="target">
          <Card>
            <Text className="text-sm font-semibold text-card-foreground">
              {peer ? peerDisplayName(peer) : 'unknown device'}
            </Text>
            <Text className="mt-0.5 text-xs text-muted-foreground">
              installs hit only this device
            </Text>
          </Card>
        </Section>

        <Section title="add app">
          <Text className="mb-2 text-xs text-muted-foreground">
            Paste a direct link to a webapp .zip bundle. The bridge fetches the
            file and pushes the bytes to the target Car Thing.
          </Text>
          <TextInput
            value={url}
            onChangeText={setUrl}
            placeholder="https://example.com/my-webapp.zip"
            placeholderTextColor="#888"
            autoCapitalize="none"
            autoCorrect={false}
            keyboardType="url"
            className="mb-2 rounded-md bg-card px-3 py-3 text-sm text-card-foreground"
          />
          <Button
            onPress={install}
            loading={installing}
            disabled={url.trim().length === 0 || !peer}
          >
            install
          </Button>
        </Section>

        <Section title={`installed (${installed.length})`}>
          {installed.length === 0 ? (
            <Empty>no apps installed on this device</Empty>
          ) : (
            installed.map(w => {
              const builtin = w.source === 'builtin';
              return (
                <Pressable
                  key={w.id}
                  onPress={() =>
                    navigation.navigate('WebappDetail', {
                      deviceId,
                      id: w.id,
                    })
                  }
                  className="mb-2"
                >
                  <Card>
                    <View className="flex-row items-center justify-between">
                      <View className="flex-1 pr-3">
                        <Text className="text-sm font-semibold text-card-foreground">
                          {w.name}
                          {builtin ? ' · built-in' : ''}
                        </Text>
                        <Text className="mt-0.5 text-xs text-muted-foreground">
                          v{w.version}
                          {w.description ? ` · ${w.description}` : ''}
                        </Text>
                      </View>
                      {!builtin ? (
                        <Button
                          variant="ghost"
                          size="sm"
                          loading={busyUninstall === w.id}
                          onPress={() => uninstall(w)}
                        >
                          remove
                        </Button>
                      ) : null}
                    </View>
                  </Card>
                </Pressable>
              );
            })
          )}
        </Section>
      </ScrollView>
    </SafeAreaView>
  );
}
