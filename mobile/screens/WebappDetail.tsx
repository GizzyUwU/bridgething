import {
  type BridgethingConfigEntry,
  type BridgethingConfigField,
  type BridgethingWebappInfo,
  peerDisplayName,
} from '@bridgething/session-react-native';
import type { NativeStackScreenProps } from '@react-navigation/native-stack';
import { useCallback, useEffect, useState } from 'react';
import {
  ActivityIndicator,
  Alert,
  Image,
  Pressable,
  ScrollView,
  Switch,
  Text,
  TextInput,
  View,
} from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { Button } from '../components/Button';
import { Card } from '../components/Card';
import { Empty, Section } from '../components/Section';
import { getSession, useSessionValue } from '../lib/session';
import type { RootStackParamList } from '../navigation';

type Props = NativeStackScreenProps<RootStackParamList, 'WebappDetail'>;

/**
 * Detail view for one installed webapp on a specific device. Keyed by
 * {deviceId, webappId} — the same app installed on two devices opens
 * as two separate detail screens with their own per-device config.
 */
export function WebappDetailScreen({ navigation, route }: Props) {
  const session = getSession();
  const { deviceId, id } = route.params;

  const peer = useSessionValue(
    s => s.cachedPeers.find(p => p.id === deviceId) ?? null,
    ['peerConnected', 'peerDisconnected'],
  );

  const [info, setInfo] = useState<BridgethingWebappInfo | null>(null);
  const [iconUri, setIconUri] = useState<string | null>(null);
  const [entries, setEntries] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState<'switch' | 'uninstall' | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoadError(null);
    try {
      const list = await session.listWebapps(deviceId);
      const match = list.find(w => w.id === id) ?? null;
      setInfo(match);
      if (match?.iconAvailable) {
        const icon = await session.webappIcon(deviceId, match.id);
        if (icon) {
          setIconUri(`data:${icon.mime ?? 'image/png'};base64,${icon.base64}`);
        }
      }
      const config = await session.listWebappConfig(deviceId, id);
      setEntries(toMap(config));
    } catch (err) {
      setLoadError(err instanceof Error ? err.message : String(err));
    }
  }, [deviceId, id, session]);

  useEffect(() => {
    load();
  }, [load]);

  const writeField = async (key: string, value: string) => {
    try {
      await session.setWebappConfigField(deviceId, id, key, value);
      setEntries(prev => ({ ...prev, [key]: value }));
    } catch (err) {
      Alert.alert(
        'Save failed',
        err instanceof Error ? err.message : String(err),
      );
    }
  };

  const resetField = async (key: string) => {
    try {
      await session.deleteWebappConfigField(deviceId, id, key);
      const fresh = await session.listWebappConfig(deviceId, id);
      setEntries(toMap(fresh));
    } catch (err) {
      Alert.alert(
        'Reset failed',
        err instanceof Error ? err.message : String(err),
      );
    }
  };

  const switchActive = async () => {
    setBusy('switch');
    try {
      await session.switchWebapp(deviceId, id);
    } catch (err) {
      Alert.alert(
        'Switch failed',
        err instanceof Error ? err.message : String(err),
      );
    } finally {
      setBusy(null);
    }
  };

  const uninstall = () => {
    if (!info || info.source === 'builtin') return;
    Alert.alert('Uninstall?', `${info.name} ${info.version}`, [
      { text: 'Cancel', style: 'cancel' },
      {
        text: 'Uninstall',
        style: 'destructive',
        onPress: async () => {
          setBusy('uninstall');
          try {
            await session.uninstallWebapp(deviceId, id);
            navigation.goBack();
          } catch (err) {
            Alert.alert(
              'Uninstall failed',
              err instanceof Error ? err.message : String(err),
            );
          } finally {
            setBusy(null);
          }
        },
      },
    ]);
  };

  if (!info) {
    return (
      <SafeAreaView
        edges={['bottom']}
        className="flex-1 items-center justify-center bg-background"
      >
        {loadError ? (
          <View className="px-6">
            <Text className="text-sm text-destructive">{loadError}</Text>
          </View>
        ) : (
          <ActivityIndicator size="small" />
        )}
      </SafeAreaView>
    );
  }

  return (
    <SafeAreaView edges={['bottom']} className="flex-1 bg-background">
      <ScrollView contentContainerClassName="px-5 pb-8 pt-2">
        <Section>
          <Card>
            <View className="flex-row items-center gap-3">
              {iconUri ? (
                <Image
                  source={{ uri: iconUri }}
                  className="h-16 w-16 rounded"
                />
              ) : (
                <View className="h-16 w-16 items-center justify-center rounded bg-muted">
                  <Text className="text-2xl font-bold text-muted-foreground">
                    {info.name.slice(0, 1).toUpperCase()}
                  </Text>
                </View>
              )}
              <View className="flex-1">
                <Text className="text-base font-semibold text-card-foreground">
                  {info.name}
                </Text>
                <Text className="mt-0.5 text-xs text-muted-foreground">
                  v{info.version}
                  {info.source === 'builtin' ? ' · built-in' : ''}
                </Text>
                {peer ? (
                  <Text className="mt-0.5 text-xs text-muted-foreground">
                    on {peerDisplayName(peer)}
                  </Text>
                ) : null}
                {info.description ? (
                  <Text className="mt-1 text-xs text-muted-foreground">
                    {info.description}
                  </Text>
                ) : null}
              </View>
            </View>
          </Card>
        </Section>

        <Section title="actions">
          <View className="flex-row gap-2">
            <Button
              onPress={switchActive}
              loading={busy === 'switch'}
              variant="primary"
            >
              switch to this
            </Button>
            {info.source !== 'builtin' ? (
              <Button
                onPress={uninstall}
                loading={busy === 'uninstall'}
                variant="destructive"
              >
                uninstall
              </Button>
            ) : null}
          </View>
        </Section>

        <Section title="settings">
          {info.config.length === 0 ? (
            <Empty>this app has no user-tunable settings</Empty>
          ) : (
            info.config.map(field => (
              <View key={field.key} className="mb-3">
                <ConfigEditor
                  field={field}
                  value={entries[field.key] ?? field.defaultValue ?? ''}
                  onCommit={value => writeField(field.key, value)}
                  onReset={() => resetField(field.key)}
                />
              </View>
            ))
          )}
        </Section>

        {info.permissions.length > 0 ? (
          <Section title="permissions">
            <Card>
              {info.permissions.map(p => (
                <Text
                  key={p}
                  className="font-mono text-xs text-card-foreground"
                >
                  {p}
                </Text>
              ))}
            </Card>
          </Section>
        ) : null}
      </ScrollView>
    </SafeAreaView>
  );
}

function ConfigEditor({
  field,
  value,
  onCommit,
  onReset,
}: {
  field: BridgethingConfigField;
  value: string;
  onCommit: (value: string) => void;
  onReset: () => void;
}) {
  return (
    <View>
      <View className="mb-1 flex-row items-baseline justify-between">
        <Text className="text-xs font-semibold uppercase tracking-widest text-muted-foreground">
          {field.label}
        </Text>
        {field.defaultValue !== undefined ? (
          <Pressable onPress={onReset}>
            <Text className="text-[11px] text-muted-foreground underline">
              reset
            </Text>
          </Pressable>
        ) : null}
      </View>
      <ConfigInput field={field} value={value} onCommit={onCommit} />
    </View>
  );
}

function ConfigInput({
  field,
  value,
  onCommit,
}: {
  field: BridgethingConfigField;
  value: string;
  onCommit: (value: string) => void;
}) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);

  switch (field.kind) {
    case 'boolean': {
      const on = value === 'true';
      return (
        <Switch
          value={on}
          onValueChange={next => onCommit(next ? 'true' : 'false')}
        />
      );
    }
    case 'enum': {
      return (
        <View className="-m-1 flex-row flex-wrap">
          {(field.choices ?? []).map(choice => {
            const selected = choice === value;
            return (
              <Pressable
                key={choice}
                onPress={() => onCommit(choice)}
                className={`m-1 rounded-md px-3 py-1.5 ${selected ? 'bg-primary' : 'bg-secondary'}`}
              >
                <Text
                  className={`text-sm font-semibold ${selected ? 'text-primary-foreground' : 'text-secondary-foreground'}`}
                >
                  {choice}
                </Text>
              </Pressable>
            );
          })}
        </View>
      );
    }
    case 'number': {
      return (
        <TextInput
          value={draft}
          onChangeText={setDraft}
          onEndEditing={() => onCommit(draft)}
          keyboardType="numeric"
          placeholderTextColor="#888"
          className="rounded-md bg-card px-3 py-3 text-sm text-card-foreground"
        />
      );
    }
    case 'secret':
    case 'string':
    default: {
      return (
        <TextInput
          value={draft}
          onChangeText={setDraft}
          onEndEditing={() => onCommit(draft)}
          autoCapitalize="none"
          autoCorrect={false}
          secureTextEntry={field.kind === 'secret'}
          placeholderTextColor="#888"
          className="rounded-md bg-card px-3 py-3 text-sm text-card-foreground"
        />
      );
    }
  }
}

function toMap(entries: BridgethingConfigEntry[]): Record<string, string> {
  const map: Record<string, string> = {};
  for (const e of entries) map[e.key] = e.value;
  return map;
}
