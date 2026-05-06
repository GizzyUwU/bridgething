import { type AdapterEvent } from '@bridgething/gateway';
import { describe, expect, test } from 'bun:test';
import { ReactNativeAdapter } from '../src';
import type { BridgethingTransport, BridgethingTransportDevice } from '../src/specs/BridgethingTransport.nitro';

class FakeTransport implements BridgethingTransport {
  // HybridObject vestiges - not exercised in tests but required by the type.
  readonly name: string = 'FakeBridgethingTransport';
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  readonly equals: any = () => false;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  readonly hashCode: any = () => 0;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  readonly toString: any = () => '[FakeBridgethingTransport]';
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  readonly dispose: any = () => {};

  startCalls = 0;
  stopCalls = 0;
  sentFrames: Array<{ deviceId: string; frame: Uint8Array }> = [];
  knownDevices: BridgethingTransportDevice[] = [];

  onConnected?: (d: BridgethingTransportDevice) => void;
  onDisconnected?: (id: string) => void;
  onBytes?: (id: string, frame: ArrayBuffer) => void;
  onError?: (id: string, description: string) => void;

  start(): Promise<void> {
    this.startCalls++;
    return Promise.resolve();
  }
  stop(): Promise<void> {
    this.stopCalls++;
    return Promise.resolve();
  }
  connect(deviceId: string): Promise<BridgethingTransportDevice> {
    return Promise.resolve({ id: deviceId, name: deviceId });
  }
  disconnect(_deviceId: string): Promise<void> {
    return Promise.resolve();
  }
  send(deviceId: string, frame: ArrayBuffer): Promise<void> {
    this.sentFrames.push({ deviceId, frame: new Uint8Array(frame) });
    return Promise.resolve();
  }
  getKnownDevices(): Promise<BridgethingTransportDevice[]> {
    return Promise.resolve(this.knownDevices);
  }
  setOnConnected(callback: (device: BridgethingTransportDevice) => void): void {
    this.onConnected = callback;
  }
  setOnDisconnected(callback: (deviceId: string) => void): void {
    this.onDisconnected = callback;
  }
  setOnBytes(callback: (deviceId: string, frame: ArrayBuffer) => void): void {
    this.onBytes = callback;
  }
  setOnError(callback: (deviceId: string, description: string) => void): void {
    this.onError = callback;
  }
}

const DEVICE: BridgethingTransportDevice = { id: 'AA:BB:CC:DD:EE:FF', name: 'Car Thing' };

describe('ReactNativeAdapter', () => {
  test('forwards connect/disconnect/bytes events', async () => {
    const transport = new FakeTransport();
    const adapter = new ReactNativeAdapter({ transport });
    await adapter.start();

    const events: AdapterEvent[] = [];
    adapter.on(e => events.push(e));

    transport.onConnected!(DEVICE);
    transport.onBytes!(DEVICE.id, new Uint8Array([0xde, 0xad]).buffer);
    transport.onDisconnected!(DEVICE.id);

    expect(events).toEqual([
      { type: 'connected', device: { id: DEVICE.id, name: DEVICE.name } },
      { type: 'bytes', deviceId: DEVICE.id, data: new Uint8Array([0xde, 0xad]) },
      { type: 'disconnected', deviceId: DEVICE.id },
    ]);
    await adapter.stop();
    expect(transport.startCalls).toBe(1);
    expect(transport.stopCalls).toBe(1);
  });

  test('send forwards bytes to the transport', async () => {
    const transport = new FakeTransport();
    const adapter = new ReactNativeAdapter({ transport });
    await adapter.start();

    await adapter.send(DEVICE.id, new Uint8Array([1, 2, 3]));
    expect(transport.sentFrames).toHaveLength(1);
    const sent = transport.sentFrames[0]!;
    expect(sent.deviceId).toBe(DEVICE.id);
    expect(Array.from(sent.frame)).toEqual([1, 2, 3]);
  });

  test('send rejects when adapter not started', async () => {
    const transport = new FakeTransport();
    const adapter = new ReactNativeAdapter({ transport });
    await expect(adapter.send(DEVICE.id, new Uint8Array([1]))).rejects.toThrow(/not started/);
  });

  test('connect forwards device record from the transport', async () => {
    const transport = new FakeTransport();
    const adapter = new ReactNativeAdapter({ transport });
    await adapter.start();
    const device = await adapter.connect(DEVICE.id);
    expect(device).toEqual({ id: DEVICE.id, name: DEVICE.id });
  });

  test('off removes a registered listener', async () => {
    const transport = new FakeTransport();
    const adapter = new ReactNativeAdapter({ transport });
    await adapter.start();

    const events: AdapterEvent[] = [];
    const listener = (e: AdapterEvent) => events.push(e);
    adapter.on(listener);
    adapter.off(listener);

    transport.onConnected!(DEVICE);
    expect(events).toHaveLength(0);
  });
});
