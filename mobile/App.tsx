import './global.css';

import {
  BridgethingSession,
  type BridgethingAuthState,
  type BridgethingProviderInfo,
  type BridgethingSessionPeer,
} from '@bridgething/session-react-native';
import { useEffect, useMemo, useRef, useState } from 'react';
import { Pressable, ScrollView, StatusBar, Text, View } from 'react-native';
import { SafeAreaProvider, SafeAreaView } from 'react-native-safe-area-context';

type LogEntry = { id: number; text: string };

export default function App() {
  const session = useMemo(() => new BridgethingSession(), []);
  const logIdRef = useRef(0);

  const [running, setRunning] = useState(false);
  const [providers, setProviders] = useState<BridgethingProviderInfo[]>([]);
  const [activeProvider, setActiveProvider] =
    useState<BridgethingProviderInfo | null>(null);
  const [authState, setAuthState] = useState<BridgethingAuthState>({
    kind: 'idle',
  });
  const [peers, setPeers] = useState<Record<string, BridgethingSessionPeer>>(
    {},
  );
  const [log, setLog] = useState<LogEntry[]>([]);

  const appendLog = (text: string) => {
    setLog(prev => [...prev, { id: logIdRef.current++, text }].slice(-100));
  };

  useEffect(() => {
    const off = session.on(event => {
      switch (event.type) {
        case 'providerChanged':
          setActiveProvider(event.provider);
          appendLog(
            event.provider
              ? `++ provider ${event.provider.id}`
              : '-- provider cleared',
          );
          break;
        case 'authStateChanged':
          setAuthState(event.state);
          appendLog(`auth ${event.state.kind}`);
          break;
        case 'peerConnected':
          setPeers(prev => ({ ...prev, [event.peer.id]: event.peer }));
          appendLog(`++ peer ${event.peer.name} (${event.peer.id})`);
          break;
        case 'peerDisconnected':
          setPeers(prev => {
            const next = { ...prev };
            delete next[event.peerId];
            return next;
          });
          appendLog(`-- peer ${event.peerId}`);
          break;
        case 'log':
          appendLog(`[${event.level}] ${event.message}`);
          break;
      }
    });
    return off;
  }, [session]);

  const start = async () => {
    if (running) return;
    try {
      await session.start();
      setRunning(true);
      const list = await session.availableProviders();
      setProviders(list);
      const current = await session.currentProvider();
      setActiveProvider(current);
      appendLog('++ session started');
    } catch (err) {
      appendLog(`!! start failed: ${errMsg(err)}`);
    }
  };

  const stop = async () => {
    if (!running) return;
    try {
      await session.stop();
    } catch (err) {
      appendLog(`!! stop failed: ${errMsg(err)}`);
    }
    setRunning(false);
    setActiveProvider(null);
    setPeers({});
    appendLog('-- session stopped');
  };

  const switchProvider = async (id: string | null) => {
    try {
      await session.setActiveProvider(id);
    } catch (err) {
      appendLog(`!! set provider failed: ${errMsg(err)}`);
    }
  };

  const peerList = Object.values(peers);

  return (
    <SafeAreaProvider>
      <SafeAreaView className="flex-1 bg-background px-4 dark:bg-background">
        <StatusBar barStyle="default" />

        <View className="mb-6 flex-row items-center justify-between">
          <View className="flex-1">
            <Text className="text-2xl font-bold text-foreground">
              bridgething
            </Text>
            <Text className="mt-0.5 text-xs text-muted-foreground">
              {running
                ? `running · ${peerList.length} peer${peerList.length === 1 ? '' : 's'}`
                : 'idle'}
            </Text>
          </View>
          <Pressable
            onPress={running ? stop : start}
            className={`rounded-md px-5 py-2.5 ${running ? 'bg-destructive' : 'bg-primary'}`}
          >
            <Text className="text-sm font-semibold text-primary-foreground">
              {running ? 'stop' : 'start'}
            </Text>
          </Pressable>
        </View>

        <Section
          title={`provider${activeProvider ? ` · ${activeProvider.displayName}` : ''}`}
        >
          {providers.length === 0 ? (
            <Empty>
              {running ? 'no providers registered' : 'press start to discover'}
            </Empty>
          ) : (
            providers.map(provider => {
              const selected = activeProvider?.id === provider.id;
              return (
                <Pressable
                  key={provider.id}
                  onPress={() => switchProvider(selected ? null : provider.id)}
                  disabled={!provider.available}
                  className={`mb-1.5 rounded-md px-3 py-2 ${selected ? 'bg-primary' : 'bg-secondary'} ${provider.available ? '' : 'opacity-50'}`}
                >
                  <Text className="text-sm font-semibold">
                    {provider.displayName}
                    {provider.available ? '' : ' (coming soon)'}
                  </Text>
                </Pressable>
              );
            })
          )}
          {authState.kind === 'pending' && (
            <View className="mt-2 rounded-md bg-card p-3">
              <Text className="text-xs text-muted-foreground">
                waiting on auth…
              </Text>
              {authState.userCode ? (
                <Text className="mt-1 font-mono text-sm font-semibold">
                  enter code: {authState.userCode}
                </Text>
              ) : null}
              {authState.verificationUrl ? (
                <Text className="mt-1 text-xs text-muted-foreground">
                  {authState.verificationUrl}
                </Text>
              ) : null}
            </View>
          )}
          {authState.kind === 'failed' && (
            <View className="mt-2 rounded-md bg-destructive/10 p-3">
              <Text className="text-xs text-destructive">
                {authState.message}
              </Text>
            </View>
          )}
        </Section>

        <Section title={`connected (${peerList.length})`}>
          {peerList.length === 0 ? (
            <Empty>no Car Things connected</Empty>
          ) : (
            peerList.map(peer => (
              <View key={peer.id} className="mb-2 rounded-md bg-card p-3">
                <Text className="text-sm font-semibold text-card-foreground">
                  {peer.name}
                </Text>
                <Text className="mt-0.5 text-xs text-muted-foreground">
                  {peer.id}
                </Text>
              </View>
            ))
          )}
        </Section>

        <Section title="log">
          <ScrollView
            className="max-h-56 rounded-md bg-muted"
            contentContainerClassName="p-2.5"
          >
            {log.map(entry => (
              <Text
                key={entry.id}
                className="font-mono text-[11px] leading-4 text-muted-foreground"
              >
                {entry.text}
              </Text>
            ))}
          </ScrollView>
        </Section>
      </SafeAreaView>
    </SafeAreaProvider>
  );
}

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <View className="mb-5">
      <Text className="mb-2 text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
        {title}
      </Text>
      {children}
    </View>
  );
}

function Empty({ children }: { children: React.ReactNode }) {
  return (
    <Text className="text-xs italic text-muted-foreground">{children}</Text>
  );
}

function errMsg(err: unknown): string {
  if (err instanceof Error) return err.message;
  return String(err);
}
