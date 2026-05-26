import { type BridgethingWebappInfo } from '@bridgething/session-react-native';
import type { NativeStackScreenProps } from '@react-navigation/native-stack';
import {
  ChevronRight,
  Download,
  Link as LinkIcon,
  Lock,
} from 'lucide-react-native';
import { useCallback, useEffect, useState } from 'react';
import { Alert, Image, ScrollView, Text, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { Button } from '../components/Button';
import { Field } from '../components/Field';
import { ListGroup } from '../components/ListGroup';
import { Press } from '../components/Press';
import { ScreenHeader } from '../components/ScreenHeader';
import { SectionEmpty, SectionHeader } from '../components/SectionHeader';
import { getSession, peerDisplayName, useSession } from '../lib/session';
import type { RootStackParamList } from '../navigation';

type Props = NativeStackScreenProps<RootStackParamList, 'WebappBrowse'>;

export function WebappBrowseScreen({ navigation, route }: Props) {
  const session = getSession();
  const deviceId = route.params.deviceId;
  const [url, setUrl] = useState('');
  const [installing, setInstalling] = useState(false);
  const [installed, setInstalled] = useState<BridgethingWebappInfo[]>([]);

  const peer = useSession(s => s.peers.find(p => p.id === deviceId) ?? null);
  const nicknames = useSession(s => s.nicknames);

  const refresh = useCallback(async () => {
    try {
      const list = await session.listWebapps(deviceId);
      setInstalled(list);
    } catch {
      // surface inline as empty
    }
  }, [deviceId, session]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // Refresh on any local install/uninstall/switch for this device.
  useEffect(() => {
    return session.subscribe(event => {
      if (event.type === 'webappsChanged' && event.deviceId === deviceId) {
        refresh();
      }
    });
  }, [deviceId, refresh, session]);

  const install = async () => {
    const trimmed = url.trim();
    if (!trimmed) return;
    setInstalling(true);
    try {
      // fetch the bundle in JS and hand the bytes to native for install.
      const response = await fetch(trimmed);
      if (!response.ok) {
        throw new Error(
          `download failed: ${response.status} ${response.statusText || ''}`.trim(),
        );
      }
      const archive = await response.arrayBuffer();
      const info = await session.installWebappFromBytes(deviceId, archive);
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

  return (
    <SafeAreaView edges={['bottom']} className="flex-1 bg-background">
      <ScrollView
        contentContainerClassName="px-5 pb-12 pt-2"
        keyboardShouldPersistTaps="handled"
      >
        <ScreenHeader
          title="install a webapp"
          subtitle={
            peer
              ? `paste a link to a webapp bundle and we'll install it on ${peerDisplayName(peer, nicknames)}.`
              : 'paste a link to a webapp bundle to install it.'
          }
        />

        <View className="mb-3">
          <Field
            label="webapp url"
            icon={LinkIcon}
            value={url}
            onChangeText={setUrl}
            clearable
            placeholder="https://example.com/my-webapp.zip"
            autoCapitalize="none"
            autoCorrect={false}
            keyboardType="url"
          />
        </View>
        <Button
          onPress={install}
          loading={installing}
          disabled={url.trim().length === 0 || !peer}
          icon={Download}
          size="lg"
        >
          install
        </Button>

        <View className="mt-10">
          <SectionHeader title="already installed" />
          {installed.length === 0 ? (
            <SectionEmpty>no webapps on this Car Thing yet</SectionEmpty>
          ) : (
            <ListGroup>
              {installed.map(w => (
                <InstalledRow
                  key={w.id}
                  webapp={w}
                  deviceId={deviceId}
                  onPress={() =>
                    navigation.navigate('WebappDetail', {
                      deviceId,
                      id: w.id,
                    })
                  }
                />
              ))}
            </ListGroup>
          )}
        </View>
      </ScrollView>
    </SafeAreaView>
  );
}

function InstalledRow({
  webapp,
  deviceId,
  onPress,
}: {
  webapp: BridgethingWebappInfo;
  deviceId: string;
  onPress: () => void;
}) {
  const session = getSession();
  const [iconUri, setIconUri] = useState<string | null>(null);

  useEffect(() => {
    if (!webapp.iconAvailable) return;
    let cancelled = false;
    (async () => {
      try {
        const icon = await session.webappIcon(deviceId, webapp.id);
        if (!cancelled && icon) {
          setIconUri(icon.fileUri);
        }
      } catch {
        /* non-fatal */
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [deviceId, session, webapp.iconAvailable, webapp.id]);

  const builtin = webapp.source === 'builtin';
  return (
    <Press onPress={onPress} fade={false} scaleTo={1}>
      <View className="flex-row items-center gap-3 px-4 py-3.5">
        <View className="h-11 w-11 items-center justify-center overflow-hidden rounded-xl bg-secondary">
          {iconUri ? (
            <Image source={{ uri: iconUri }} className="h-11 w-11" />
          ) : (
            <Text className="text-[16px] font-extrabold text-foreground">
              {webapp.name.slice(0, 1).toUpperCase()}
            </Text>
          )}
        </View>
        <View className="flex-1">
          <View className="flex-row items-center gap-2">
            <Text
              className="flex-shrink text-[15px] font-semibold text-foreground"
              numberOfLines={1}
            >
              {webapp.name}
            </Text>
            {builtin ? (
              <View className="flex-row items-center gap-1 rounded-full bg-secondary px-2 py-0.5">
                <Lock size={9} color="hsl(215 14% 38%)" strokeWidth={2.6} />
                <Text className="text-[10px] font-bold uppercase tracking-[0.14em] text-muted-foreground">
                  built-in
                </Text>
              </View>
            ) : null}
          </View>
          <Text
            className="mt-0.5 text-[12.5px] text-muted-foreground"
            numberOfLines={1}
          >
            v{webapp.version}
            {webapp.description ? ` · ${webapp.description}` : ''}
          </Text>
        </View>
        <ChevronRight size={18} color="hsl(215 14% 60%)" strokeWidth={2.2} />
      </View>
    </Press>
  );
}
