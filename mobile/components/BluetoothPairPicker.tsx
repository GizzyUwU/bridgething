import type {
  BridgethingBtDevice,
} from '@bridgething/session-react-native';
import { BellRing, Check, RefreshCcw } from 'lucide-react-native';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { ActivityIndicator, Alert, Text, View } from 'react-native';

import { Button } from './Button';
import { Press } from './Press';
import {
  openSystemBluetoothSettings,
  requestPairPermissions,
} from '../lib/pair-permissions';
import { getSession } from '../lib/session';

/**
 * In-app Bluetooth pair picker (android-only). Lists bonded Car Things
 * the OS already knows about, plus discovered (in-pairing-mode) devices
 * from a live BR/EDR scan. Tapping a row triggers `createBond()` natively
 * — Android pops the system pairing dialog and we listen for the bond
 * state change to resolve the promise.
 *
 * Filters the discovery firehose to bridgething-class devices by default
 * (the Car Thing advertises CoD 0x7c0000 + name "Car Thing"), but lets
 * the user toggle "show all" so devices that haven't reported their
 * class yet still show up.
 */
export function BluetoothPairPicker({
  onPaired,
}: {
  onPaired?: (device: BridgethingBtDevice) => void;
}) {
  const session = getSession();

  const [bonded, setBonded] = useState<BridgethingBtDevice[]>([]);
  const [discovered, setDiscovered] = useState<BridgethingBtDevice[]>([]);
  const [scanning, setScanning] = useState(false);
  const [pairing, setPairing] = useState<string | null>(null);
  const [showAll, setShowAll] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const list = await session.listBondedBluetoothDevices();
      setBonded(list);
    } catch (err) {
      console.warn('[bridgething] listBonded failed', err);
    }
  }, [session]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // Subscribe to discovery + bond events for the lifetime of the picker.
  useEffect(() => {
    const off = session.subscribe(event => {
      if (event.type === 'btDiscoveryEvent') {
        switch (event.event.kind) {
          case 'started':
            setScanning(true);
            setDiscovered([]);
            return;
          case 'finished':
            setScanning(false);
            return;
          case 'found':
            if (event.event.device) {
              const device = event.event.device;
              setDiscovered(prev => {
                if (prev.some(d => d.address === device.address)) return prev;
                return [...prev, device];
              });
            }
            return;
          case 'failed':
            setScanning(false);
            if (event.event.reason) {
              Alert.alert('scan failed', event.event.reason);
            }
            return;
        }
      }
      if (event.type === 'btBondStateChanged') {
        const device = event.device;
        setBonded(prev => {
          const others = prev.filter(d => d.address !== device.address);
          return device.bondState === 'bonded' ? [...others, device] : others;
        });
        // Only rebuild the discovered list when the changed device is
        // actually in it; otherwise we churn a new array reference + force
        // a re-render on every bond event for unrelated devices.
        setDiscovered(prev =>
          prev.some(d => d.address === device.address)
            ? prev.map(d => (d.address === device.address ? device : d))
            : prev,
        );
        if (device.bondState === 'bonded') {
          onPaired?.(device);
        }
      }
    });
    return () => {
      off();
      session.stopBluetoothDiscovery().catch(() => {});
    };
  }, [session, onPaired]);

  const startScan = async () => {
    const perm = await requestPairPermissions();
    if (perm === 'blocked') {
      Alert.alert(
        'permission needed',
        'bridgething needs nearby-devices permission to scan. open settings to grant it.',
        [
          { text: 'cancel', style: 'cancel' },
          { text: 'open settings', onPress: openSystemBluetoothSettings },
        ],
      );
      return;
    }
    if (perm !== 'granted') return;
    try {
      await session.startBluetoothDiscovery();
    } catch (err) {
      Alert.alert(
        'scan failed',
        err instanceof Error ? err.message : String(err),
      );
    }
  };

  const pair = async (device: BridgethingBtDevice) => {
    if (pairing) return;
    setPairing(device.address);
    try {
      await session.stopBluetoothDiscovery();
      const state = await session.pairBluetoothDevice(device.address);
      if (state !== 'bonded') {
        Alert.alert('pairing failed', 'the device declined the bond.');
      }
    } catch (err) {
      Alert.alert(
        'pairing failed',
        err instanceof Error ? err.message : String(err),
      );
    } finally {
      setPairing(null);
      refresh();
    }
  };

  const bondedFiltered = useMemo(
    () => bonded.filter(d => showAll || d.isCarThing),
    [bonded, showAll],
  );
  const discoveredFiltered = useMemo(() => {
    const bondedAddrs = new Set(bondedFiltered.map(d => d.address));
    return discovered.filter(
      d => !bondedAddrs.has(d.address) && (showAll || d.isCarThing),
    );
  }, [discovered, bondedFiltered, showAll]);

  return (
    <View className="gap-5">
      {bondedFiltered.length > 0 && (
        <View className="gap-2">
          <Text className="text-[12px] font-bold uppercase tracking-[0.18em] text-muted-foreground">
            already paired
          </Text>
          {bondedFiltered.map(device => (
            <DeviceRow
              key={device.address}
              device={device}
              pairingAddress={pairing}
              onPress={() => pair(device)}
            />
          ))}
        </View>
      )}

      <View className="gap-2">
        <View className="flex-row items-center justify-between">
          <Text className="text-[12px] font-bold uppercase tracking-[0.18em] text-muted-foreground">
            discover new
          </Text>
          <Press onPress={() => setShowAll(v => !v)} hitSlop={8}>
            <Text className="text-[12px] font-medium text-primary">
              {showAll ? 'only Car Things' : 'show all devices'}
            </Text>
          </Press>
        </View>

        {discoveredFiltered.length === 0 && !scanning ? (
          <View className="rounded-2xl border border-border bg-surface px-4 py-6">
            <Text className="text-center text-[13px] text-muted-foreground">
              tap scan and put your Car Thing in pairing mode.
            </Text>
          </View>
        ) : (
          discoveredFiltered.map(device => (
            <DeviceRow
              key={device.address}
              device={device}
              pairingAddress={pairing}
              onPress={() => pair(device)}
            />
          ))
        )}

        <Button
          onPress={startScan}
          loading={scanning}
          variant="tonal"
          size="md"
          icon={RefreshCcw}
        >
          {scanning ? 'scanning…' : 'scan'}
        </Button>
      </View>
    </View>
  );
}

function DeviceRow({
  device,
  pairingAddress,
  onPress,
}: {
  device: BridgethingBtDevice;
  pairingAddress: string | null;
  onPress: () => void;
}) {
  const isPairing = pairingAddress === device.address;
  const isBonded = device.bondState === 'bonded';
  const isBonding = device.bondState === 'bonding' || isPairing;
  const disabled = pairingAddress != null && pairingAddress !== device.address;
  const subtitle = subtitleFor(device, isBonded, isBonding);
  return (
    <Press onPress={onPress} disabled={disabled || isBonded || isBonding} scaleTo={0.98}>
      <View
        className={`flex-row items-center gap-3 rounded-2xl border bg-surface px-4 py-3.5 ${
          isBonded ? 'border-primary' : 'border-border'
        } ${disabled ? 'opacity-60' : ''}`}
      >
        <View
          className={`h-11 w-11 items-center justify-center rounded-2xl ${
            isBonded ? 'bg-primary' : 'bg-secondary'
          }`}
        >
          <BellRing
            size={18}
            color={isBonded ? 'white' : 'hsl(210 22% 11%)'}
            strokeWidth={2.4}
          />
        </View>
        <View className="flex-1">
          <Text className="text-[15px] font-semibold text-foreground">
            {device.name ?? device.address}
          </Text>
          <Text className="mt-0.5 text-[12px] text-muted-foreground">
            {subtitle}
          </Text>
        </View>
        {isBonding ? (
          <ActivityIndicator size="small" />
        ) : isBonded ? (
          <View className="h-7 w-7 items-center justify-center rounded-full bg-primary">
            <Check size={14} color="white" strokeWidth={3} />
          </View>
        ) : null}
      </View>
    </Press>
  );
}

function subtitleFor(
  device: BridgethingBtDevice,
  isBonded: boolean,
  isBonding: boolean,
): string {
  if (isBonded) return 'paired';
  if (isBonding) return 'pairing…';
  if (device.isCarThing) return 'Car Thing';
  return device.address;
}
