import type { BridgethingProviderInfo } from '@bridgething/session-react-native';
import type { NativeStackScreenProps } from '@react-navigation/native-stack';
import { ArrowRight, BellRing, Check, ChevronLeft } from 'lucide-react-native';
import { useEffect, useState } from 'react';
import {
  Alert,
  Platform,
  ScrollView,
  Text,
  useWindowDimensions,
  View,
} from 'react-native';
import Animated, {
  Easing,
  useAnimatedStyle,
  useSharedValue,
  withTiming,
} from 'react-native-reanimated';
import { SafeAreaView } from 'react-native-safe-area-context';

import { Button } from '../components/Button';
import { HeroPulse } from '../components/HeroPulse';
import { IconBadge } from '../components/IconBadge';
import { PagerDots } from '../components/PagerDots';
import { PendingAuth } from '../components/PendingAuth';
import { Press } from '../components/Press';
import { getSession, useSession } from '../lib/session';
import { setSetupCompleted } from '../lib/storage';
import type { RootStackParamList } from '../navigation';

type Props = NativeStackScreenProps<RootStackParamList, 'Setup'>;

const STEP_COUNT = 2;

export function SetupScreen({ navigation, route }: Props) {
  const session = getSession();
  const { width } = useWindowDimensions();

  // Step is mirrored to route params so it survives a screen remount
  // (react-native-screens detaches us while a system activity like the
  // CDM picker is in the foreground).
  const initialStep =
    route.params?.step ?? (route.params?.startAt === 'pair' ? 1 : 0);
  const [step, setStepState] = useState(initialStep);
  const setStep = (next: number | ((prev: number) => number)) => {
    setStepState(prev => {
      const value = typeof next === 'function' ? next(prev) : next;
      navigation.setParams({ step: value });
      return value;
    });
  };
  const [providers, setProviders] = useState<BridgethingProviderInfo[]>([]);
  const [busyProviderId, setBusyProviderId] = useState<string | null>(null);
  const [pairBusy, setPairBusy] = useState(false);

  const provider = useSession(s => s.provider);
  const authState = useSession(s => s.authState);
  const ancsStatus = useSession(s => s.ancsAuthStatus);
  const peers = useSession(s => s.peers);

  // iOS uses ANCS auth state as the pair signal (LE bond from ASK
  // landed). Android has no ANCS - we treat any live RFCOMM peer as
  // paired, since CompanionDeviceManager only resolves successfully
  // after the system has already completed the BR/EDR bond.
  const paired =
    Platform.OS === 'android' ? peers.length > 0 : ancsStatus !== 'unknown';
  const signedIn = authState.kind === 'authenticated' && provider != null;

  useEffect(() => {
    let cancelled = false;
    (async () => {
      const list = await session.availableProviders();
      if (!cancelled) setProviders(list);
    })();
    return () => {
      cancelled = true;
    };
  }, [session]);

  // Auto-advance to step 2 the moment tokens land for the first time.
  useEffect(() => {
    if (authState.kind === 'idle') setBusyProviderId(null);
    if (authState.kind === 'authenticated' && step === 0) {
      const t = setTimeout(() => setStep(1), 350);
      return () => clearTimeout(t);
    }
    return undefined;
  }, [authState.kind, step]);

  const offset = useSharedValue(0);
  useEffect(() => {
    offset.value = withTiming(-step * width, {
      duration: 360,
      easing: Easing.out(Easing.cubic),
    });
  }, [step, width, offset]);

  const pagesStyle = useAnimatedStyle(() => ({
    transform: [{ translateX: offset.value }],
  }));

  const pickProvider = async (provider: BridgethingProviderInfo) => {
    if (busyProviderId || !provider.available) return;
    setBusyProviderId(provider.id);
    try {
      await session.setActiveProvider(provider.id);
    } catch {
      // setActiveProvider routes failures via authStateChanged.
    } finally {
      setBusyProviderId(null);
    }
  };

  const cancelAuth = async () => {
    await session.cancelAuth();
    setBusyProviderId(null);
  };

  const pair = async () => {
    if (pairBusy) return;
    setPairBusy(true);
    try {
      if (Platform.OS === 'android') {
        // CompanionDeviceManager picker. Returns the chosen accessory
        // or null on cancel - no error handling needed.
        await session.presentPairPicker();
        return;
      }
      const result = await session.enableAncsNotifications();
      if (result.kind === 'failed') {
        Alert.alert(
          'pairing failed',
          result.message ?? 'something went wrong while pairing.',
        );
      }
    } catch (err) {
      Alert.alert(
        'pairing failed',
        err instanceof Error ? err.message : String(err),
      );
    } finally {
      setPairBusy(false);
    }
  };

  const finish = () => {
    setSetupCompleted(true);
    navigation.replace('Dashboard');
  };

  return (
    <SafeAreaView edges={['top', 'bottom']} className="flex-1 bg-background">
      <View className="flex-row items-center justify-between px-5 py-3">
        {step > 0 ? (
          <Press
            onPress={() => setStep(s => Math.max(0, s - 1))}
            className="-ml-1 h-9 w-9 items-center justify-center rounded-full"
            scaleTo={0.92}
          >
            <ChevronLeft size={22} color="hsl(210 22% 11%)" strokeWidth={2.4} />
          </Press>
        ) : (
          <View className="h-9 w-9" />
        )}
        <PagerDots count={STEP_COUNT} index={step} />
        <View className="h-9 w-9" />
      </View>

      <View className="flex-1 overflow-hidden">
        <Animated.View
          className="flex-1 flex-row"
          style={[{ width: width * STEP_COUNT }, pagesStyle]}
        >
          <Page width={width}>
            <SignInPage
              providers={providers}
              activeProvider={provider ?? null}
              authState={authState}
              busyProviderId={busyProviderId}
              onPickProvider={pickProvider}
              onCancelAuth={cancelAuth}
              onContinue={() => setStep(1)}
              signedIn={signedIn}
            />
          </Page>
          <Page width={width}>
            <PairPage
              paired={paired}
              busy={pairBusy}
              onPair={pair}
              onFinish={finish}
            />
          </Page>
        </Animated.View>
      </View>
    </SafeAreaView>
  );
}

function Page({
  children,
  width,
}: {
  children: React.ReactNode;
  width: number;
}) {
  return <View style={{ width }}>{children}</View>;
}

function SignInPage({
  providers,
  activeProvider,
  authState,
  busyProviderId,
  onPickProvider,
  onCancelAuth,
  onContinue,
  signedIn,
}: {
  providers: BridgethingProviderInfo[];
  activeProvider: BridgethingProviderInfo | null;
  authState: import('@bridgething/session-react-native').BridgethingAuthState;
  busyProviderId: string | null;
  onPickProvider: (p: BridgethingProviderInfo) => void;
  onCancelAuth: () => void;
  onContinue: () => void;
  signedIn: boolean;
}) {
  return (
    <ScrollView
      contentContainerClassName="px-7 pb-8 pt-6"
      showsVerticalScrollIndicator={false}
    >
      <Text className="mb-1.5 text-[12px] font-bold uppercase tracking-[0.18em] text-primary">
        step 1 of 2
      </Text>
      <Text
        className="text-foreground"
        style={{
          fontFamily: 'Outfit-Medium',
          fontSize: 30,
          lineHeight: 34,
          letterSpacing: -0.9,
        }}
      >
        sign in to your music
      </Text>
      <Text className="mt-2 text-[14px] leading-[20px] text-muted-foreground">
        your Car Thing plays music from your phone. sign in once here and you
        won&apos;t have to think about it again.
      </Text>

      <View className="mt-6 gap-2.5">
        {providers.length === 0 ? (
          <View className="rounded-2xl border border-border bg-surface px-4 py-6">
            <Text className="text-center text-[13px] text-muted-foreground">
              starting up…
            </Text>
          </View>
        ) : (
          providers.map(p => {
            const selected = activeProvider?.id === p.id;
            const busy = busyProviderId === p.id;
            return (
              <ProviderTile
                key={p.id}
                name={p.displayName}
                id={p.id}
                selected={selected}
                busy={busy}
                authStatus={selected ? authState.kind : 'idle'}
                disabled={!p.available || (!!busyProviderId && !busy)}
                comingSoon={!p.available}
                onPress={() => onPickProvider(p)}
              />
            );
          })
        )}
      </View>

      <View className="mt-4">
        <PendingAuth
          state={authState}
          onCancel={authState.kind === 'pending' ? onCancelAuth : undefined}
          onRetry={
            authState.kind === 'failed' && activeProvider
              ? () => onPickProvider(activeProvider)
              : undefined
          }
        />
      </View>

      <View className="mt-8 gap-2">
        <Button
          onPress={onContinue}
          icon={ArrowRight}
          size="lg"
          variant={signedIn ? 'primary' : 'tonal'}
        >
          {signedIn ? 'continue' : 'skip for now'}
        </Button>
      </View>
    </ScrollView>
  );
}

function ProviderTile({
  name,
  id,
  selected,
  busy,
  authStatus,
  disabled,
  comingSoon,
  onPress,
}: {
  name: string;
  id: string;
  selected: boolean;
  busy: boolean;
  authStatus: 'idle' | 'pending' | 'authenticated' | 'failed';
  disabled: boolean;
  comingSoon: boolean;
  onPress: () => void;
}) {
  const initial = name.slice(0, 1).toUpperCase();
  const subtitle = comingSoon
    ? 'coming soon'
    : busy
      ? 'opening…'
      : !selected
        ? id
        : authStatus === 'authenticated'
          ? 'signed in'
          : authStatus === 'failed'
            ? 'sign-in failed'
            : authStatus === 'pending'
              ? 'finish in your browser'
              : id;
  const showCheck = selected && authStatus === 'authenticated';
  return (
    <Press onPress={onPress} disabled={disabled} scaleTo={0.98}>
      <View
        className={`flex-row items-center gap-3 rounded-2xl border bg-surface px-4 py-3.5 ${
          selected ? 'border-primary' : 'border-border'
        } ${disabled ? 'opacity-60' : ''}`}
        style={
          selected
            ? {
                shadowColor: 'hsl(199 100% 44%)',
                shadowOpacity: 0.18,
                shadowRadius: 14,
                shadowOffset: { width: 0, height: 6 },
                elevation: 2,
              }
            : undefined
        }
      >
        <View
          className={`h-11 w-11 items-center justify-center rounded-2xl ${
            selected ? 'bg-primary' : 'bg-secondary'
          }`}
        >
          <Text
            className={`text-[16px] font-extrabold ${
              selected ? 'text-primary-foreground' : 'text-secondary-foreground'
            }`}
          >
            {initial}
          </Text>
        </View>
        <View className="flex-1">
          <Text className="text-[15px] font-semibold text-foreground">
            {name}
          </Text>
          <Text className="mt-0.5 text-[12px] text-muted-foreground">
            {subtitle}
          </Text>
        </View>
        {showCheck ? (
          <View className="h-7 w-7 items-center justify-center rounded-full bg-primary">
            <Check size={14} color="white" strokeWidth={3} />
          </View>
        ) : null}
      </View>
    </Press>
  );
}

function PairPage({
  paired,
  busy,
  onPair,
  onFinish,
}: {
  paired: boolean;
  busy: boolean;
  onPair: () => void;
  onFinish: () => void;
}) {
  return (
    <ScrollView
      contentContainerClassName="flex-1 px-7 pb-8 pt-6"
      showsVerticalScrollIndicator={false}
    >
      <View className="flex-1">
        <Text className="mb-1.5 text-[12px] font-bold uppercase tracking-[0.18em] text-primary">
          step 2 of 2
        </Text>
        <Text
          className="text-foreground"
          style={{
            fontFamily: 'Outfit-Medium',
            fontSize: 30,
            lineHeight: 34,
            letterSpacing: -0.9,
          }}
        >
          pair your Car Thing
        </Text>
        <Text className="mt-2 text-[14px] leading-[20px] text-muted-foreground">
          turn on your Car Thing and tap pair. you&apos;ll see a system picker -
          choose your device to finish.
        </Text>

        <View className="my-10 items-center">
          {paired ? (
            <IconBadge icon={Check} tint="success" size={88} />
          ) : (
            <View className="items-center justify-center">
              <HeroPulse tint="primary" />
              <View className="absolute">
                <BellRing size={28} color="white" strokeWidth={2.2} />
              </View>
            </View>
          )}
        </View>
      </View>

      <View className="gap-2.5">
        {paired ? (
          <Button onPress={onFinish} icon={ArrowRight} size="lg">
            open dashboard
          </Button>
        ) : (
          <>
            <Button onPress={onPair} loading={busy} size="lg" icon={BellRing}>
              pair
            </Button>
            <Button onPress={onFinish} variant="ghost" size="md">
              skip — i&apos;ll pair later
            </Button>
          </>
        )}
      </View>
    </ScrollView>
  );
}
