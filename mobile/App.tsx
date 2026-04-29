import './global.css';

import { type BridgethingTransportDevice, ReactNativeAdapter } from '@bridgething/adapter-react-native';
import { BridgethingGateway, type GatewayEvent } from '@bridgething/gateway';
import {
  type BridgeThingMeta,
  type BridgeToGatewayMsg,
  type GatewayMeta,
  LIB_VERSION,
  LIBBRIDGETHING_VERSION,
  LogLevel,
  newUuidBytes,
} from '@bridgething/lib';
import { StatusBar } from 'expo-status-bar';
import { useEffect, useMemo, useRef, useState } from 'react';
import { type Permission, PermissionsAndroid, Platform, Pressable, ScrollView, Text, View } from 'react-native';
import { SafeAreaProvider, SafeAreaView } from 'react-native-safe-area-context';

const GATEWAY_META: GatewayMeta = {
  adapterVersion: 'v0.1.0',
  appVersion: '0.1.0',
  appName: 'bridgething-mobile',
  libbridgethingVersion: LIBBRIDGETHING_VERSION,
  libVersion: LIB_VERSION,
  osName: Platform.OS,
};

type ConnectedPeer = {
  id: string;
  name: string;
  bridgeMeta?: BridgeThingMeta;
};

type LogEntry = { id: number; text: string };

export default function App() {
  const adapter = useMemo(() => new ReactNativeAdapter(), []);
  const gateway = useMemo(() => new BridgethingGateway(adapter, { logLevel: LogLevel.Trace }), [adapter]);
  const logIdRef = useRef(0);

  const [running, setRunning] = useState(false);
  const [knownDevices, setKnownDevices] = useState<BridgethingTransportDevice[]>([]);
  const [connectedPeers, setConnectedPeers] = useState<Record<string, ConnectedPeer>>({});
  const [log, setLog] = useState<LogEntry[]>([]);

  const appendLog = (text: string) => {
    setLog(prev => [...prev, { id: logIdRef.current++, text }].slice(-100));
  };

  useEffect(() => {
    const unsubscribe = gateway.on(event => handleGatewayEvent(event));
    return () => {
      unsubscribe();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [gateway]);

  useEffect(() => {
    if (!running) return;
    const interval = setInterval(() => {
      try {
        setKnownDevices(adapter.getKnownDevices());
      } catch {
        // adapter not started yet
      }
    }, 2000);
    return () => clearInterval(interval);
  }, [running, adapter]);

  const handleGatewayEvent = (event: GatewayEvent) => {
    switch (event.type) {
      case 'connected':
        appendLog(`++ connected ${event.device.name} (${event.device.id})`);
        setConnectedPeers(prev => ({
          ...prev,
          [event.device.id]: { id: event.device.id, name: event.device.name },
        }));
        break;
      case 'disconnected':
        appendLog(`-- disconnected ${event.deviceId}`);
        setConnectedPeers(prev => {
          const next = { ...prev };
          delete next[event.deviceId];
          return next;
        });
        break;
      case 'message':
        void handleBridgeMessage(event.deviceId, event.message);
        break;
      case 'decodeError':
        appendLog(`!! decode error on ${event.deviceId}: ${event.description}`);
        break;
    }
  };

  const handleBridgeMessage = async (deviceId: string, msg: BridgeToGatewayMsg) => {
    switch (msg.data.type) {
      case 'version': {
        const meta = msg.data.data;
        appendLog(`<< version from ${deviceId}: ${meta.appName} ${meta.appVersion}`);
        setConnectedPeers(prev => {
          const peer = prev[deviceId];
          if (!peer) return prev;
          return { ...prev, [deviceId]: { ...peer, bridgeMeta: meta } };
        });
        try {
          await gateway.send(deviceId, {
            id: newUuidBytes(),
            meta: { kind: 'event' },
            data: { type: 'version', data: GATEWAY_META },
          });
          appendLog(`>> sent gateway version to ${deviceId}`);
        } catch (err) {
          appendLog(`!! send version failed: ${errMsg(err)}`);
        }
        break;
      }
      case 'forward':
        appendLog(`<< forward (${msg.data.data.encoding}) from ${deviceId}`);
        break;
      case 'file':
        appendLog(`<< file ${msg.data.data.event} from ${deviceId}`);
        break;
      case 'ack':
      case 'done':
        appendLog(`<< ${msg.data.type} from ${deviceId}`);
        break;
    }
  };

  const start = async () => {
    if (running) return;
    if (Platform.OS === 'android') {
      const ok = await ensureAndroidBluetoothPermissions();
      if (!ok) {
        appendLog('!! bluetooth permissions denied');
        return;
      }
    }
    try {
      await gateway.start();
      setRunning(true);
      setKnownDevices(adapter.getKnownDevices());
      appendLog('++ gateway started');
    } catch (err) {
      appendLog(`!! gateway start failed: ${errMsg(err)}`);
    }
  };

  const stop = async () => {
    if (!running) return;
    try {
      await gateway.stop();
    } catch (err) {
      appendLog(`!! gateway stop failed: ${errMsg(err)}`);
    }
    setRunning(false);
    setKnownDevices([]);
    setConnectedPeers({});
    appendLog('-- gateway stopped');
  };

  const connectDevice = async (deviceId: string) => {
    try {
      const device = await adapter.connect(deviceId);
      appendLog(`>> connect requested ${device.name} (${device.id})`);
    } catch (err) {
      appendLog(`!! connect failed: ${errMsg(err)}`);
    }
  };

  const disconnectDevice = async (deviceId: string) => {
    try {
      await gateway.disconnect(deviceId);
    } catch (err) {
      appendLog(`!! disconnect failed: ${errMsg(err)}`);
    }
  };

  const peers = Object.values(connectedPeers);

  return (
    <SafeAreaProvider>
      <SafeAreaView className="flex-1 bg-background px-4 dark:bg-background">
        <StatusBar style="auto" />

        <View className="mb-6 flex-row items-center justify-between">
          <View className="flex-1">
            <Text className="text-2xl font-bold text-foreground">bridgething</Text>
            <Text className="mt-0.5 text-xs text-muted-foreground">
              {running ? `running · ${peers.length} connected` : 'idle'}
            </Text>
          </View>
          <Pressable
            onPress={running ? stop : start}
            className={`rounded-md px-5 py-2.5 ${running ? 'bg-destructive' : 'bg-primary'}`}>
            <Text className="text-sm font-semibold text-primary-foreground">{running ? 'stop' : 'start'}</Text>
          </Pressable>
        </View>

        <Section title={`known devices (${knownDevices.length})`}>
          {knownDevices.length === 0 ? (
            <Empty>
              {running ? 'no peers - pair a Car Thing in system Bluetooth settings' : 'press start to scan'}
            </Empty>
          ) : (
            knownDevices.map(d => (
              <Pressable
                key={d.id}
                onPress={() => connectDevice(d.id)}
                className="mb-1.5 rounded-md bg-secondary px-3 py-2 active:opacity-70">
                <Text className="text-sm font-semibold text-secondary-foreground">{d.name}</Text>
                <Text className="mt-0.5 text-xs text-muted-foreground">{d.id}</Text>
              </Pressable>
            ))
          )}
        </Section>

        <Section title={`connected (${peers.length})`}>
          {peers.length === 0 ? (
            <Empty>no active sessions</Empty>
          ) : (
            peers.map(peer => (
              <View key={peer.id} className="mb-2 rounded-md bg-card p-3">
                <View className="flex-row items-center justify-between">
                  <Text className="text-sm font-semibold text-card-foreground">{peer.name}</Text>
                  <Pressable onPress={() => disconnectDevice(peer.id)}>
                    <Text className="text-xs text-destructive">disconnect</Text>
                  </Pressable>
                </View>
                {peer.bridgeMeta ? (
                  <View className="mt-2">
                    <MetaLine label="app" value={`${peer.bridgeMeta.appName} ${peer.bridgeMeta.appVersion}`} />
                    <MetaLine label="os" value={`${peer.bridgeMeta.osName} ${peer.bridgeMeta.osVersion}`} />
                    <MetaLine label="image" value={peer.bridgeMeta.imageBuildId} />
                    <MetaLine label="model" value={peer.bridgeMeta.modelName} />
                    <MetaLine label="serial" value={peer.bridgeMeta.serialNumber} />
                  </View>
                ) : (
                  <Empty>waiting for version…</Empty>
                )}
              </View>
            ))
          )}
        </Section>

        <Section title="log">
          <ScrollView className="max-h-56 rounded-md bg-muted" contentContainerClassName="p-2.5">
            {log.map(entry => (
              <Text key={entry.id} className="font-mono text-[11px] leading-4 text-muted-foreground">
                {entry.text}
              </Text>
            ))}
          </ScrollView>
        </Section>
      </SafeAreaView>
    </SafeAreaProvider>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <View className="mb-5">
      <Text className="mb-2 text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">{title}</Text>
      {children}
    </View>
  );
}

function Empty({ children }: { children: React.ReactNode }) {
  return <Text className="text-xs italic text-muted-foreground">{children}</Text>;
}

function MetaLine({ label, value }: { label: string; value: string }) {
  return (
    <View className="mb-0.5 flex-row">
      <Text className="w-16 text-xs text-muted-foreground">{label}</Text>
      <Text className="flex-1 text-xs text-foreground">{value || '-'}</Text>
    </View>
  );
}

async function ensureAndroidBluetoothPermissions(): Promise<boolean> {
  const permissions: Permission[] = [];
  if (Platform.Version && Number(Platform.Version) >= 31) {
    permissions.push(PermissionsAndroid.PERMISSIONS.BLUETOOTH_CONNECT, PermissionsAndroid.PERMISSIONS.BLUETOOTH_SCAN);
  }
  if (permissions.length === 0) return true;
  const result = await PermissionsAndroid.requestMultiple(permissions);
  return permissions.every(p => result[p] === PermissionsAndroid.RESULTS.GRANTED);
}

function errMsg(err: unknown): string {
  if (err instanceof Error) return err.message;
  return String(err);
}
