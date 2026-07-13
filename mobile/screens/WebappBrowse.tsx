import { type BridgethingWebappInfo } from '@bridgething/session-react-native';
import type { NativeStackScreenProps } from '@react-navigation/native-stack';
import {
  ChevronRight,
  Download,
  Link as LinkIcon,
  Lock,
  Store as StoreIcon,
} from 'lucide-react-native';
import { useState } from 'react';
import { Alert, Text, View } from 'react-native';

import { Button } from '../components/Button';
import { Field } from '../components/Field';
import { ListGroup } from '../components/ListGroup';
import { Press } from '../components/Press';
import { ScreenHeader } from '../components/ScreenHeader';
import { ScrollScreen } from '../components/ScrollScreen';
import { SectionEmpty, SectionHeader } from '../components/SectionHeader';
import { WebappIcon } from '../components/WebappIcon';
import { getSession, peerDisplayName, useSession } from '../lib/session';
import { useWebapps } from '../lib/webapps';
import type { RootStackParamList } from '../navigation';

type Props = NativeStackScreenProps<RootStackParamList, 'WebappBrowse'>;

export function WebappBrowseScreen({ navigation, route }: Props) {
  const session = getSession();
  const deviceId = route.params.deviceId;
  const [url, setUrl] = useState('');
  const [installing, setInstalling] = useState(false);
  const { list: installed } = useWebapps(deviceId);

  const peer = useSession(s => s.peers.find(p => p.id === deviceId) ?? null);
  const ledger = useSession(s => s.ledger);

  const install = async () => {
    const trimmed = url.trim();
    if (!trimmed) return;
    setInstalling(true);
    try {
      const info = await session.installWebappFromUri(deviceId, trimmed);
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
    <ScrollScreen>
      <ScreenHeader
        title="install a webapp"
        subtitle={
          peer
            ? `paste a link to a webapp bundle and we'll install it on ${peerDisplayName(peer, ledger)}.`
            : 'paste a link to a webapp bundle to install it.'
        }
      />

      <View className="mb-6">
        <Button
          onPress={() => navigation.navigate('Store', { deviceId })}
          disabled={!peer}
          icon={StoreIcon}
          size="lg"
        >
          browse the app store
        </Button>
      </View>

      <SectionHeader title="or install from a link" />
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
    </ScrollScreen>
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
  const builtin = webapp.source === 'builtin';
  return (
    <Press onPress={onPress} fade={false} scaleTo={1}>
      <View className="flex-row items-center gap-3 px-4 py-3.5">
        <WebappIcon
          deviceId={deviceId}
          id={webapp.id}
          iconHash={webapp.iconHash}
          name={webapp.name}
          size={44}
          fallbackTextClass="text-[16px] font-extrabold text-foreground"
        />
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
