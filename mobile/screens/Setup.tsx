import type { BridgethingProviderInfo } from '@bridgething/session-react-native';
import type { NativeStackScreenProps } from '@react-navigation/native-stack';
import { useEffect, useState } from 'react';
import { Alert, Pressable, ScrollView, Text, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { Button } from '../components/Button';
import { PendingAuth } from '../components/PendingAuth';
import { Empty, Section } from '../components/Section';
import { getSession, useSessionEvents, useSessionValue } from '../lib/session';
import type { RootStackParamList } from '../navigation';

type Props = NativeStackScreenProps<RootStackParamList, 'Setup'>;

/**
 * First-launch screen. Three blocking steps in order:
 *  1. Pick a provider and complete sign-in.
 *  2. Pair ANCS over AccessorySetupKit (skippable on iOS < 18 / Android,
 *     dismissable with confirmation otherwise).
 *  3. Tap "done" to advance to the Dashboard.
 *
 * Once a provider is authenticated AND ancsAuthStatus is `authorized` or
 * `unsupported`, the "done" button enables. The user can still skip
 * ANCS pairing — the daemon will keep working without notifications.
 */
export function SetupScreen({ navigation }: Props) {
  const session = getSession();

  const [running, setRunning] = useState(false);
  const [providers, setProviders] = useState<BridgethingProviderInfo[]>([]);
  const [busyProviderId, setBusyProviderId] = useState<string | null>(null);
  const [ancsBusy, setAncsBusy] = useState(false);
  const [ancsSkipped, setAncsSkipped] = useState(false);

  const activeProvider = useSessionValue(
    s => s.cachedProvider,
    ['providerChanged'],
  );
  const authState = useSessionValue(
    s => s.cachedAuthState,
    ['authStateChanged'],
  );
  const ancsStatus = useSessionValue(
    s => s.cachedAncsAuthStatus,
    ['ancsAuthStatusChanged'],
  );

  // Boot the session on first mount.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        await session.start();
        if (cancelled) return;
        setRunning(true);
        const list = await session.availableProviders();
        if (!cancelled) setProviders(list);
      } catch (err) {
        Alert.alert(
          'Could not start session',
          err instanceof Error ? err.message : String(err),
        );
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [session]);

  // Once authenticated AND ANCS settled, advance.
  const ancsResolved =
    ancsStatus === 'authorized' || ancsStatus === 'unauthorized'
      ? ancsStatus === 'authorized'
      : false;
  const ancsSkippable = ancsStatus === 'unauthorized';
  const setupComplete =
    authState.kind === 'authenticated' &&
    activeProvider != null &&
    (ancsResolved || ancsSkipped);

  useSessionEvents(event => {
    if (event.type === 'authStateChanged' && event.state.kind === 'idle') {
      // glue detached - back to provider picker
      setBusyProviderId(null);
    }
  });

  const pickProvider = async (provider: BridgethingProviderInfo) => {
    if (busyProviderId || !provider.available) return;
    setBusyProviderId(provider.id);
    try {
      await session.setActiveProvider(provider.id);
    } catch (err) {
      // setActiveProvider already routes failures through authStateChanged
      // so we just clear local busy here.
    } finally {
      setBusyProviderId(null);
    }
  };

  const cancelAuth = async () => {
    await session.cancelAuth();
    setBusyProviderId(null);
  };

  const enableAncs = async () => {
    if (ancsBusy) return;
    setAncsBusy(true);
    try {
      const result = await session.enableAncsNotifications();
      if (result.kind === 'unsupported') {
        // Older iOS / Android - allow advance.
        setAncsSkipped(true);
        Alert.alert(
          'Notifications unavailable',
          'AccessorySetupKit requires iOS 18 or later. The Car Thing will work without notification mirroring.',
        );
      } else if (result.kind === 'failed') {
        Alert.alert(
          'Pairing failed',
          result.message ?? 'AccessorySetupKit reported an error.',
        );
      } else if (result.kind === 'cancelled') {
        // user dismissed picker - leave as-is, they can try again or skip
      }
    } catch (err) {
      Alert.alert(
        'Pairing failed',
        err instanceof Error ? err.message : String(err),
      );
    } finally {
      setAncsBusy(false);
    }
  };

  const skipAncs = () => {
    Alert.alert(
      'Skip notifications?',
      'Without ANCS pairing the Car Thing will not show iPhone notifications. You can re-enable this later in Settings.',
      [
        { text: 'Cancel', style: 'cancel' },
        {
          text: 'Skip',
          style: 'destructive',
          onPress: () => setAncsSkipped(true),
        },
      ],
    );
  };

  const proceed = () => navigation.replace('Dashboard');

  return (
    <SafeAreaView edges={['top', 'bottom']} className="flex-1 bg-background">
      <ScrollView contentContainerClassName="px-5 pb-8 pt-4">
        <View className="mb-6">
          <Text className="text-3xl font-bold text-foreground">
            bridgething
          </Text>
          <Text className="mt-1 text-sm text-muted-foreground">
            {running ? 'Get your Car Thing set up' : 'Starting up the bridge…'}
          </Text>
        </View>

        <Section title="step 1 · sign in">
          {providers.length === 0 ? (
            <Empty>
              {running ? 'no providers registered in this build' : 'one moment'}
            </Empty>
          ) : (
            providers.map(p => {
              const selected = activeProvider?.id === p.id;
              const busy = busyProviderId === p.id;
              return (
                <Pressable
                  key={p.id}
                  onPress={() => pickProvider(p)}
                  disabled={!p.available || !!busyProviderId}
                  className={`mb-1.5 rounded-md px-3 py-3 ${selected ? 'bg-primary' : 'bg-secondary'} ${p.available ? '' : 'opacity-50'}`}
                >
                  <Text
                    className={`text-base font-semibold ${selected ? 'text-primary-foreground' : 'text-secondary-foreground'}`}
                  >
                    {p.displayName}
                    {p.available ? '' : ' (coming soon)'}
                    {busy ? ' · signing in…' : ''}
                  </Text>
                </Pressable>
              );
            })
          )}
          <View className="mt-2">
            <PendingAuth
              state={authState}
              onCancel={authState.kind === 'pending' ? cancelAuth : undefined}
              onRetry={
                authState.kind === 'failed' && activeProvider
                  ? () => pickProvider(activeProvider)
                  : undefined
              }
            />
          </View>
        </Section>

        <Section title="step 2 · iPhone notifications">
          <Text className="mb-3 text-xs text-muted-foreground">
            Pair the Car Thing with iOS so ANCS can mirror your notifications.
            Skippable; the device works without it.
          </Text>
          <View className="mb-2 flex-row items-center gap-2">
            <View
              className={`h-2 w-2 rounded-full ${ancsResolved ? 'bg-primary' : ancsStatus === 'probing' ? 'bg-muted' : 'bg-muted-foreground'}`}
            />
            <Text className="text-xs text-muted-foreground">
              {ancsLabel(ancsStatus)}
            </Text>
          </View>
          <View className="flex-row gap-2">
            <Button
              onPress={enableAncs}
              loading={ancsBusy}
              disabled={!running || ancsResolved}
              variant={ancsResolved ? 'secondary' : 'primary'}
            >
              {ancsResolved ? 'enabled' : 'enable on Car Thing'}
            </Button>
            {!ancsResolved && !ancsSkipped && ancsSkippable ? (
              <Button variant="ghost" onPress={skipAncs}>
                skip
              </Button>
            ) : null}
          </View>
        </Section>

        <View className="mt-4">
          <Button onPress={proceed} disabled={!setupComplete} variant="primary">
            done
          </Button>
        </View>
      </ScrollView>
    </SafeAreaView>
  );
}

function ancsLabel(status: string): string {
  switch (status) {
    case 'authorized':
      return 'paired and authorized';
    case 'unauthorized':
      return 'paired, awaiting authorization';
    case 'probing':
      return 'checking…';
    default:
      return 'not paired';
  }
}
