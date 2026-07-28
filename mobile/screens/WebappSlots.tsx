import type {
  BridgethingWebappInfo,
  BridgethingWebappSlot,
  BridgethingWebappSlots,
} from '@bridgething/session-react-native';
import type { NativeStackScreenProps } from '@react-navigation/native-stack';
import { Check, Layers, LayoutGrid, Lock } from 'lucide-react-native';
import { useCallback, useEffect, useState } from 'react';
import { ActivityIndicator, Alert, Text, View } from 'react-native';

import { ListGroup } from '../components/ListGroup';
import { Press } from '../components/Press';
import { ScreenHeader } from '../components/ScreenHeader';
import { ScrollScreen } from '../components/ScrollScreen';
import { SectionEmpty, SectionHeader } from '../components/SectionHeader';
import { WebappIcon } from '../components/WebappIcon';
import { getSession } from '../lib/session';
import { useWebapps } from '../lib/webapps';
import type { RootStackParamList } from '../navigation';

type Props = NativeStackScreenProps<RootStackParamList, 'WebappSlots'>;

export function WebappSlotsScreen({ route }: Props) {
  const session = getSession();
  const deviceId = route.params.deviceId;
  const { list } = useWebapps(deviceId);

  const [slots, setSlots] = useState<BridgethingWebappSlots | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<BridgethingWebappSlot | null>(null);

  useEffect(() => {
    let cancelled = false;
    session
      .getWebappSlots(deviceId)
      .then(next => !cancelled && setSlots(next))
      .catch(
        err =>
          !cancelled &&
          setError(err instanceof Error ? err.message : String(err)),
      );
    return () => {
      cancelled = true;
    };
  }, [session, deviceId]);

  const assign = useCallback(
    async (slot: BridgethingWebappSlot, id?: string) => {
      if (busy) return;
      setBusy(slot);
      try {
        setSlots(await session.setWebappSlot(deviceId, slot, id));
      } catch (err) {
        Alert.alert(
          'Could not change slot',
          err instanceof Error ? err.message : String(err),
        );
      } finally {
        setBusy(null);
      }
    },
    [busy, session, deviceId],
  );

  const launchers = list.filter(
    w => w.role === 'launcher' && w.source === 'installed',
  );
  const overlays = list.filter(
    w => w.overlayHash != null && w.source === 'installed',
  );

  if (error) {
    return (
      <ScrollScreen>
        <ScreenHeader title="home screen and overlay" subtitle={error} />
      </ScrollScreen>
    );
  }

  if (!slots) {
    return (
      <ScrollScreen>
        <View className="items-center py-12">
          <ActivityIndicator />
        </View>
      </ScrollScreen>
    );
  }

  return (
    <ScrollScreen>
      <ScreenHeader
        title="home screen and overlay"
        subtitle="pick which installed app provides each. choosing built-in always works, so this is also how you recover from one that misbehaves."
      />

      <SectionHeader title="home screen" />
      <ListGroup>
        <BuiltinRow
          label="built-in hub"
          detail="the launcher that ships with bridgething"
          icon={LayoutGrid}
          selected={slots.launcher == null}
          busy={busy === 'launcher'}
          onPress={() => assign('launcher')}
        />
        {launchers.map(w => (
          <CandidateRow
            key={w.id}
            webapp={w}
            deviceId={deviceId}
            selected={slots.launcher === w.id}
            busy={busy === 'launcher'}
            onPress={() => assign('launcher', w.id)}
          />
        ))}
      </ListGroup>
      {launchers.length === 0 ? (
        <SectionEmpty>no installed app declares itself a launcher</SectionEmpty>
      ) : null}

      <View className="mt-10">
        <SectionHeader title="system overlay" />
        <ListGroup>
          <BuiltinRow
            label="built-in overlay"
            detail="notifications, calls, pairing, volume"
            icon={Layers}
            selected={slots.overlay == null}
            busy={busy === 'overlay'}
            onPress={() => assign('overlay')}
          />
          {overlays.map(w => (
            <CandidateRow
              key={w.id}
              webapp={w}
              deviceId={deviceId}
              selected={slots.overlay === w.id}
              busy={busy === 'overlay'}
              onPress={() => assign('overlay', w.id)}
            />
          ))}
        </ListGroup>
        {overlays.length === 0 ? (
          <SectionEmpty>no installed app ships an overlay</SectionEmpty>
        ) : null}
      </View>
    </ScrollScreen>
  );
}

function SelectionMark({
  selected,
  busy,
}: {
  selected: boolean;
  busy: boolean;
}) {
  if (busy) return <ActivityIndicator size="small" />;
  if (!selected) return <View className="h-[18px] w-[18px]" />;
  return <Check size={18} color="hsl(215 14% 38%)" strokeWidth={2.6} />;
}

function BuiltinRow({
  label,
  detail,
  icon: Icon,
  selected,
  busy,
  onPress,
}: {
  label: string;
  detail: string;
  icon: typeof LayoutGrid;
  selected: boolean;
  busy: boolean;
  onPress: () => void;
}) {
  return (
    <Press onPress={onPress} fade={false} scaleTo={1}>
      <View className="flex-row items-center gap-3 px-4 py-3.5">
        <View className="h-11 w-11 items-center justify-center rounded-xl bg-secondary">
          <Icon size={20} color="hsl(215 14% 38%)" strokeWidth={2.2} />
        </View>
        <View className="flex-1">
          <View className="flex-row items-center gap-2">
            <Text
              className="flex-shrink text-[15px] font-semibold text-foreground"
              numberOfLines={1}
            >
              {label}
            </Text>
            <View className="flex-row items-center gap-1 rounded-full bg-secondary px-2 py-0.5">
              <Lock size={9} color="hsl(215 14% 38%)" strokeWidth={2.6} />
              <Text className="text-[10px] font-bold uppercase tracking-[0.14em] text-muted-foreground">
                built-in
              </Text>
            </View>
          </View>
          <Text
            className="mt-0.5 text-[12.5px] text-muted-foreground"
            numberOfLines={1}
          >
            {detail}
          </Text>
        </View>
        <SelectionMark selected={selected} busy={busy} />
      </View>
    </Press>
  );
}

function CandidateRow({
  webapp,
  deviceId,
  selected,
  busy,
  onPress,
}: {
  webapp: BridgethingWebappInfo;
  deviceId: string;
  selected: boolean;
  busy: boolean;
  onPress: () => void;
}) {
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
          <Text
            className="text-[15px] font-semibold text-foreground"
            numberOfLines={1}
          >
            {webapp.name}
          </Text>
          <Text
            className="mt-0.5 text-[12.5px] text-muted-foreground"
            numberOfLines={1}
          >
            v{webapp.version}
            {webapp.description ? ` · ${webapp.description}` : ''}
          </Text>
        </View>
        <SelectionMark selected={selected} busy={busy} />
      </View>
    </Press>
  );
}
