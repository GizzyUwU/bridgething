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
import {
  type Permission,
  PermissionsAndroid,
  Platform,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  View,
} from 'react-native';

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
    setLog(prev => {
      const next = [...prev, { id: logIdRef.current++, text }];
      return next.slice(-100);
    });
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
        // adapter not started yet - ignore
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
    <View style={styles.container}>
      <StatusBar style="auto" />

      <View style={styles.header}>
        <Text style={styles.title}>bridgething</Text>
        <Text style={styles.subtitle}>{running ? `running · ${peers.length} connected` : 'idle'}</Text>
        <Pressable
          style={[styles.button, running ? styles.buttonStop : styles.buttonStart]}
          onPress={running ? stop : start}>
          <Text style={styles.buttonText}>{running ? 'stop' : 'start'}</Text>
        </Pressable>
      </View>

      <Section title={`known devices (${knownDevices.length})`}>
        {knownDevices.length === 0 ? (
          <Text style={styles.empty}>
            {running ? 'no peers - pair a Car Thing in system Bluetooth settings' : 'press start to scan'}
          </Text>
        ) : (
          knownDevices.map(d => (
            <Pressable key={d.id} style={styles.row} onPress={() => connectDevice(d.id)}>
              <Text style={styles.rowName}>{d.name}</Text>
              <Text style={styles.rowDetail}>{d.id}</Text>
            </Pressable>
          ))
        )}
      </Section>

      <Section title={`connected (${peers.length})`}>
        {peers.length === 0 ? (
          <Text style={styles.empty}>no active sessions</Text>
        ) : (
          peers.map(peer => (
            <View key={peer.id} style={styles.peer}>
              <View style={styles.peerHeader}>
                <Text style={styles.rowName}>{peer.name}</Text>
                <Pressable onPress={() => disconnectDevice(peer.id)}>
                  <Text style={styles.disconnect}>disconnect</Text>
                </Pressable>
              </View>
              {peer.bridgeMeta ? (
                <View style={styles.metaBlock}>
                  <MetaLine label="app" value={`${peer.bridgeMeta.appName} ${peer.bridgeMeta.appVersion}`} />
                  <MetaLine label="os" value={`${peer.bridgeMeta.osName} ${peer.bridgeMeta.osVersion}`} />
                  <MetaLine label="image" value={peer.bridgeMeta.imageBuildId} />
                  <MetaLine label="model" value={peer.bridgeMeta.modelName} />
                  <MetaLine label="serial" value={peer.bridgeMeta.serialNumber} />
                </View>
              ) : (
                <Text style={styles.empty}>waiting for version…</Text>
              )}
            </View>
          ))
        )}
      </Section>

      <Section title="log">
        <ScrollView style={styles.logScroll} contentContainerStyle={styles.logContent}>
          {log.map(entry => (
            <Text key={entry.id} style={styles.logLine}>
              {entry.text}
            </Text>
          ))}
        </ScrollView>
      </Section>
    </View>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <View style={styles.section}>
      <Text style={styles.sectionTitle}>{title}</Text>
      {children}
    </View>
  );
}

function MetaLine({ label, value }: { label: string; value: string }) {
  return (
    <View style={styles.metaLine}>
      <Text style={styles.metaLabel}>{label}</Text>
      <Text style={styles.metaValue}>{value || '-'}</Text>
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

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#101114',
    paddingTop: 64,
    paddingHorizontal: 16,
  },
  header: {
    marginBottom: 24,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
  },
  title: { color: '#f8f8f2', fontSize: 24, fontWeight: '700' },
  subtitle: { color: '#90a4ae', fontSize: 13, marginLeft: 12, flex: 1 },
  button: {
    paddingHorizontal: 20,
    paddingVertical: 10,
    borderRadius: 8,
  },
  buttonStart: { backgroundColor: '#2dd4bf' },
  buttonStop: { backgroundColor: '#f87171' },
  buttonText: { color: '#0b0c0f', fontSize: 14, fontWeight: '700' },
  section: { marginBottom: 20 },
  sectionTitle: {
    color: '#90a4ae',
    fontSize: 11,
    letterSpacing: 1.2,
    textTransform: 'uppercase',
    marginBottom: 8,
  },
  empty: { color: '#52606d', fontSize: 13, fontStyle: 'italic' },
  row: {
    paddingVertical: 8,
    paddingHorizontal: 12,
    backgroundColor: '#1a1c20',
    borderRadius: 6,
    marginBottom: 6,
  },
  rowName: { color: '#f8f8f2', fontSize: 14, fontWeight: '600' },
  rowDetail: { color: '#7a8794', fontSize: 11, marginTop: 2 },
  peer: {
    paddingVertical: 10,
    paddingHorizontal: 12,
    backgroundColor: '#1a1c20',
    borderRadius: 6,
    marginBottom: 8,
  },
  peerHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  disconnect: { color: '#f87171', fontSize: 12 },
  metaBlock: { marginTop: 8 },
  metaLine: { flexDirection: 'row', marginBottom: 2 },
  metaLabel: { color: '#7a8794', fontSize: 12, width: 64 },
  metaValue: { color: '#cfd8dc', fontSize: 12, flex: 1 },
  logScroll: { maxHeight: 220, backgroundColor: '#0a0b0d', borderRadius: 6 },
  logContent: { padding: 10 },
  logLine: { color: '#cfd8dc', fontFamily: 'Menlo', fontSize: 11, lineHeight: 16 },
});
