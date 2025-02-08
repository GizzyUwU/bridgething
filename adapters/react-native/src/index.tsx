import { type Adapter, type AdapterCallback } from '@bridgething/gateway';
import { BRIDGETHING_CHARACTERISTIC_UUID, BRIDGETHING_SERVICE_UUID, Logger, LogLevel } from '@bridgething/lib';
import { gzip, inflate } from 'pako';
import { PermissionsAndroid, Platform } from 'react-native';
import {
  LogLevel as BLELogLevel,
  BleManager,
  State as BLEState,
  type BleError,
  type Subscription as BLESubscription,
  type Characteristic,
  type Device,
  type DeviceId,
} from 'react-native-ble-plx';

class ReactNativeAdapter implements Adapter {
  private readonly logger: Logger;
  private readonly manager: BleManager;

  private readonly callbacks: AdapterCallback[] = [];
  private readonly subscriptions: Record<string, BLESubscription> = {};

  private ready: boolean = false;
  private readonly devices: Record<DeviceId, Device> = {};

  constructor(logLevel: LogLevel = LogLevel.Log) {
    this.logger = new Logger('Adapter', logLevel);
    this.manager = new BleManager({});
    void this.manager.setLogLevel(logLevelToBleLogLevel(logLevel));
    this.logger.debug('initializing ble manager');

    this.subscriptions.state = this.manager.onStateChange(state => this.handleStateUpdate(state));
  }

  /**
   * blocks until the bluetooth adapter is ready, then starts scan.
   * @throws THIS WILL THROW IF BLUETOOTH PERMISSION IS DENIED
   */
  async init() {
    const hasPermission = await this.requestBluetoothPermission();
    if (!hasPermission) throw new Error('failed to get bluetooth permissions!');

    await this.manager.enable();

    const state = await this.manager.state();
    this.ready = state === BLEState.PoweredOn;
    // TODO: is there a better way to do this?
    while (!this.ready) await new Promise(resolve => setTimeout(resolve, 100));

    await this.scanOn();
  }

  on = (callback: AdapterCallback) => this.callbacks.push(callback);

  scanOn = () =>
    this.manager.startDeviceScan(
      [BRIDGETHING_SERVICE_UUID],
      { legacyScan: false },
      (error, device) => void this.handleDeviceFound(error, device),
    );

  scanOff = () => this.manager.stopDeviceScan();

  /** @throws THIS WILL THROW IF THE DEVICE IS NOT KNOWN/CONNECTED */
  async disconnect(deviceId: DeviceId) {
    const device = this.devices[deviceId];
    if (!device) throw new Error('device not known!');

    await device.cancelConnection();
  }

  /** @throws THIS WILL THROW IF THE DEVICE IS NOT KNOWN/CONNECTED OR IF SEND FAILS */
  async send(deviceId: DeviceId, message: Uint8Array) {
    const device = this.devices[deviceId];
    if (!device) throw new Error('device not known!');

    const data = gzip(message);

    await device.writeCharacteristicWithoutResponseForService(
      BRIDGETHING_SERVICE_UUID,
      BRIDGETHING_CHARACTERISTIC_UUID,
      bytesToBase64(data),
    );
  }

  private handleRecv(error: BleError | null, char: Characteristic | null) {
    if (error) this.logger.error('characteristic read error: ', error);
    if (!char) return;

    this.logger.trace('new characteristic update: ', char);

    const data = char.value;
    if (!data) return this.logger.warn('received characteristic update with no data!');

    const decompressed = inflate(base64ToBytes(data));
    this.callbacks.map(c => c({ type: 'data', deviceId: char.deviceID, data: decompressed }));
  }

  private async handleDeviceFound(error: BleError | null, device: Device | null) {
    if (error) this.logger.error('scan error: ', error);
    if (!device) return;

    this.logger.debug('found device running bridgething: ', device);
    const connected = await device.isConnected();
    if (!connected) await device.connect();

    this.subscriptions[`${device.id}:char`] = device.monitorCharacteristicForService(
      BRIDGETHING_SERVICE_UUID,
      BRIDGETHING_CHARACTERISTIC_UUID,
      (error, char) => void this.handleRecv(error, char),
    );
  }

  private handleStateUpdate(state: BLEState) {
    this.logger.debug('new bluetooth state update: ', state);
    if (state === BLEState.PoweredOn) this.ready = true;
  }

  private requestBluetoothPermission = async () => {
    if (Platform.OS === 'ios') return true;

    if (Platform.OS === 'android' && PermissionsAndroid.PERMISSIONS.ACCESS_FINE_LOCATION) {
      const apiLevel = parseInt(Platform.Version.toString(), 10);

      if (apiLevel < 31) {
        const granted = await PermissionsAndroid.request(PermissionsAndroid.PERMISSIONS.ACCESS_FINE_LOCATION);
        return granted === PermissionsAndroid.RESULTS.GRANTED;
      }
      if (PermissionsAndroid.PERMISSIONS.BLUETOOTH_SCAN && PermissionsAndroid.PERMISSIONS.BLUETOOTH_CONNECT) {
        const result = await PermissionsAndroid.requestMultiple([
          PermissionsAndroid.PERMISSIONS.BLUETOOTH_SCAN,
          PermissionsAndroid.PERMISSIONS.BLUETOOTH_CONNECT,
          PermissionsAndroid.PERMISSIONS.ACCESS_FINE_LOCATION,
        ]);

        return (
          result['android.permission.BLUETOOTH_CONNECT'] === PermissionsAndroid.RESULTS.GRANTED &&
          result['android.permission.BLUETOOTH_SCAN'] === PermissionsAndroid.RESULTS.GRANTED &&
          result['android.permission.ACCESS_FINE_LOCATION'] === PermissionsAndroid.RESULTS.GRANTED
        );
      }
    }

    return false;
  };
}

function bytesToBase64(data: Uint8Array): string {
  return btoa(String.fromCharCode.apply(null, Array.from(data)));
}

function base64ToBytes(data: string): Uint8Array {
  const binary = atob(data);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

function logLevelToBleLogLevel(logLevel: LogLevel) {
  switch (logLevel) {
    case LogLevel.Trace:
      return BLELogLevel.Verbose;
    case LogLevel.Debug:
      return BLELogLevel.Debug;
    case LogLevel.Log:
      return BLELogLevel.Info;
    case LogLevel.Warn:
      return BLELogLevel.Warning;
    case LogLevel.Error:
      return BLELogLevel.Error;
    default:
      return BLELogLevel.None;
  }
}

export { ReactNativeAdapter };
