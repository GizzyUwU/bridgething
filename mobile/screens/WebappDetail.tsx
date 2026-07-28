import {
  type BridgethingConfigEntry,
  type BridgethingConfigField,
} from '@bridgething/session-react-native';
import type { NativeStackScreenProps } from '@react-navigation/native-stack';
import {
  ArrowUpCircle,
  Bell,
  Cable,
  ChevronRight,
  Globe,
  LayoutGrid,
  type LucideIcon,
  MapPin,
  Mic,
  Play,
  RotateCcw,
  Shield,
  SlidersHorizontal,
  Speaker,
  Trash2,
  Wifi,
} from 'lucide-react-native';
import { useCallback, useEffect, useState } from 'react';
import {
  ActivityIndicator,
  Alert,
  Pressable,
  Switch,
  Text,
  TextInput,
  View,
} from 'react-native';

import { Button } from '../components/Button';
import { ListGroup } from '../components/ListGroup';
import { ListRow } from '../components/ListRow';
import { Pill } from '../components/Pill';
import { Press } from '../components/Press';
import { ScrollScreen } from '../components/ScrollScreen';
import { SectionEmpty, SectionHeader } from '../components/SectionHeader';
import { WebappIcon } from '../components/WebappIcon';
import { useUpdates } from '../lib/catalog';
import { getSession, peerDisplayName, useSession } from '../lib/session';
import { useWebapps } from '../lib/webapps';
import type { RootStackParamList } from '../navigation';

type Props = NativeStackScreenProps<RootStackParamList, 'WebappDetail'>;

export function WebappDetailScreen({ navigation, route }: Props) {
  const session = getSession();
  const { deviceId, id } = route.params;

  const peer = useSession(s => s.peers.find(p => p.id === deviceId) ?? null);
  const ledger = useSession(s => s.ledger);

  const { list } = useWebapps(deviceId);
  const info = list.find(w => w.id === id) ?? null;
  const update =
    useUpdates(deviceId).find(
      u => u.appId.toLowerCase() === id.toLowerCase(),
    ) ?? null;
  const [entries, setEntries] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState<'switch' | 'uninstall' | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  const loadConfig = useCallback(async () => {
    setLoadError(null);
    try {
      const config = await session.listWebappConfig(deviceId, id);
      setEntries(toMap(config));
    } catch (err) {
      setLoadError(err instanceof Error ? err.message : String(err));
    }
  }, [deviceId, id, session]);

  useEffect(() => {
    loadConfig();
  }, [loadConfig]);

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
      <View className="flex-1 items-center justify-center bg-background">
        {loadError ? (
          <View className="px-6">
            <Text className="text-center text-[14px] text-destructive">
              {loadError}
            </Text>
          </View>
        ) : (
          <ActivityIndicator size="small" color="hsl(199 100% 44%)" />
        )}
      </View>
    );
  }

  const builtin = info.source === 'builtin';

  return (
    <ScrollScreen contentContainerStyle={{ paddingTop: 12 }}>
      <View
        className="mb-6 flex-row items-center gap-4 rounded-2xl border border-border bg-surface p-4"
        style={{
          shadowColor: '#000',
          shadowOpacity: 0.06,
          shadowRadius: 14,
          shadowOffset: { width: 0, height: 6 },
        }}
      >
        <WebappIcon
          deviceId={deviceId}
          id={info.id}
          iconHash={info.iconHash}
          name={info.name}
          size={64}
          radiusClass="rounded-2xl"
          fallbackTextClass="text-[24px] font-extrabold text-foreground"
        />
        <View className="flex-1">
          <Text
            className="text-[20px] font-extrabold leading-[24px] text-foreground"
            numberOfLines={2}
            style={{ letterSpacing: -0.4 }}
          >
            {info.name}
          </Text>
          <Text className="mt-0.5 font-mono text-[12px] text-muted-foreground">
            v{info.version}
          </Text>
          <View className="mt-2 flex-row flex-wrap gap-1.5">
            {builtin ? (
              <Pill tone="neutral">built-in</Pill>
            ) : (
              <Pill tone="primary">installed</Pill>
            )}
            {peer ? (
              <Pill tone="neutral" dot={false}>
                {peerDisplayName(peer, ledger)}
              </Pill>
            ) : null}
          </View>
        </View>
      </View>

      {info.description ? (
        <Text className="mb-6 px-1 text-[14px] leading-[20px] text-muted-foreground">
          {info.description}
        </Text>
      ) : null}

      {update ? (
        <Press
          onPress={() =>
            navigation.navigate('StoreApp', {
              deviceId,
              appId: update.appId,
              sourceUrl: update.sourceUrl,
            })
          }
          className="mb-6"
          scaleTo={0.98}
        >
          <View className="flex-row items-center gap-3 rounded-2xl border border-primary/30 bg-primary-soft px-4 py-3">
            <ArrowUpCircle
              size={18}
              color="hsl(199 100% 44%)"
              strokeWidth={2.2}
            />
            <View className="flex-1">
              <Text className="text-[13px] font-semibold text-foreground">
                update available
              </Text>
              <Text className="mt-0.5 font-mono text-[12px] text-muted-foreground">
                v{update.installedVersion} → v{update.target.version}
              </Text>
            </View>
            <ChevronRight
              size={16}
              color="hsl(215 14% 50%)"
              strokeWidth={2.4}
            />
          </View>
        </Press>
      ) : null}

      <View className="mb-8 flex-row gap-2">
        <View className="flex-1">
          <Button
            onPress={switchActive}
            loading={busy === 'switch'}
            variant="primary"
            icon={Play}
          >
            switch to this
          </Button>
        </View>
        {!builtin ? (
          <View className="flex-1">
            <Button
              onPress={uninstall}
              loading={busy === 'uninstall'}
              variant="destructive"
              icon={Trash2}
            >
              uninstall
            </Button>
          </View>
        ) : null}
      </View>

      {info.role === 'launcher' || info.overlayHash ? (
        <View className="mb-8">
          <Button
            onPress={() => navigation.navigate('WebappSlots', { deviceId })}
            variant="secondary"
            icon={LayoutGrid}
          >
            {info.role === 'launcher' && info.overlayHash
              ? 'use as home screen or overlay'
              : info.role === 'launcher'
                ? 'use as home screen'
                : 'use as system overlay'}
          </Button>
        </View>
      ) : null}

      {info.settingsHash ? (
        <View className="mb-8">
          <Button
            onPress={() =>
              navigation.navigate('WebappSettings', {
                deviceId,
                id,
                name: info.name,
              })
            }
            variant="secondary"
            icon={SlidersHorizontal}
          >
            open {info.name} settings
          </Button>
        </View>
      ) : null}

      {info.config.length > 0 ? (
        <View className="mb-8">
          <SectionHeader title="settings" hint="changes save on commit" />
          <View className="gap-3">
            {info.config.map(field => (
              <ConfigEditor
                key={field.key}
                field={field}
                value={entries[field.key] ?? field.defaultValue ?? ''}
                onCommit={value => writeField(field.key, value)}
                onReset={() => resetField(field.key)}
              />
            ))}
          </View>
        </View>
      ) : (
        <View className="mb-8">
          <SectionHeader title="settings" />
          <SectionEmpty>this app has no user-tunable settings</SectionEmpty>
        </View>
      )}

      {info.permissions.length > 0 ? (
        <View>
          <SectionHeader
            title="what this webapp can do"
            hint="granted automatically; capabilities your phone offers are in Settings → Advanced"
          />
          <ListGroup>
            {info.permissions.map(p => {
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
        </View>
      ) : null}
    </ScrollScreen>
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
    <View
      className="rounded-2xl border border-border bg-surface p-4"
      style={{
        shadowColor: '#000',
        shadowOpacity: 0.04,
        shadowRadius: 8,
        shadowOffset: { width: 0, height: 3 },
      }}
    >
      <View className="mb-2 flex-row items-center justify-between">
        <Text className="flex-1 text-[12px] font-bold uppercase tracking-[0.18em] text-muted-foreground">
          {field.label}
        </Text>
        {field.defaultValue !== undefined ? (
          <Pressable
            onPress={onReset}
            hitSlop={10}
            className="flex-row items-center gap-1"
          >
            <RotateCcw size={11} color="hsl(199 100% 44%)" strokeWidth={2.4} />
            <Text className="text-[11px] font-semibold uppercase tracking-[0.14em] text-primary">
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
        <View className="flex-row items-center justify-between">
          <Text className="text-[14px] text-foreground">
            {on ? 'enabled' : 'disabled'}
          </Text>
          <Switch
            value={on}
            onValueChange={next => onCommit(next ? 'true' : 'false')}
          />
        </View>
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
                className={`m-1 rounded-full px-3 py-1.5 ${selected ? 'bg-primary' : 'bg-secondary'}`}
              >
                <Text
                  className={`text-[13px] font-semibold ${selected ? 'text-primary-foreground' : 'text-secondary-foreground'}`}
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
          placeholderTextColor="hsl(215 14% 55%)"
          className="rounded-xl bg-surface-subtle px-3 py-3 text-[15px] text-foreground"
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
          placeholderTextColor="hsl(215 14% 55%)"
          className="rounded-xl bg-surface-subtle px-3 py-3 text-[15px] text-foreground"
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
      return {
        icon: Bell,
        title: 'show iPhone notifications',
      };
    case 'audio.tts':
    case 'audio':
      return {
        icon: Speaker,
        title: 'play sound',
        subtitle: 'plays through your phone',
      };
    case 'mic':
      return {
        icon: Mic,
        title: 'use the Car Thing microphone',
      };
    default:
      return { icon: Shield, title: perm };
  }
}
