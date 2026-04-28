import {
  type BridgeToGatewayMsg,
  Codec,
  type GatewayMeta,
  type GatewayToBridgeMsg,
  newUuidBytes,
} from '@bridgething/lib';
import { describe, expect, test } from 'bun:test';
import {
  type Adapter,
  type AdapterEvent,
  type AdapterListener,
  BridgethingGateway,
  type Device,
  type GatewayEvent,
} from '../src';

class MockAdapter implements Adapter {
  readonly listeners: Set<AdapterListener> = new Set();
  readonly sentFrames: Array<{ deviceId: string; frame: Uint8Array }> = [];
  startCalled = false;
  stopCalled = false;

  on(listener: AdapterListener): void {
    this.listeners.add(listener);
  }
  off(listener: AdapterListener): void {
    this.listeners.delete(listener);
  }
  async start(): Promise<void> {
    this.startCalled = true;
  }
  async stop(): Promise<void> {
    this.stopCalled = true;
  }
  async disconnect(_: string): Promise<void> {}
  async send(deviceId: string, frame: Uint8Array): Promise<void> {
    this.sentFrames.push({ deviceId, frame });
  }

  emit(event: AdapterEvent): void {
    for (const l of this.listeners) l(event);
  }
}

const meta: GatewayMeta = {
  adapterVersion: 'v0.0.0',
  libVersion: 'v0.0.0',
  libbridgethingVersion: 'v0.0.0',
  appName: 'test',
  appVersion: 'v0.0.0',
  osName: 'test',
};

const DEVICE: Device = { id: 'test-device', name: 'Test Device' };

function recordEvents(gateway: BridgethingGateway): GatewayEvent[] {
  const events: GatewayEvent[] = [];
  gateway.on(event => events.push(event));
  return events;
}

describe('BridgethingGateway', () => {
  test('forwards connect and disconnect events', async () => {
    const adapter = new MockAdapter();
    const gateway = new BridgethingGateway(adapter);
    const events = recordEvents(gateway);
    await gateway.start();

    adapter.emit({ type: 'connected', device: DEVICE });
    adapter.emit({ type: 'disconnected', deviceId: DEVICE.id });

    expect(events).toEqual([
      { type: 'connected', device: DEVICE },
      { type: 'disconnected', deviceId: DEVICE.id },
    ]);
    await gateway.stop();
  });

  test('encodes outbound message via codec', async () => {
    const adapter = new MockAdapter();
    const gateway = new BridgethingGateway(adapter);
    await gateway.start();

    const msg: GatewayToBridgeMsg = {
      id: newUuidBytes(),
      meta: { kind: 'event' },
      data: { type: 'version', data: meta },
    };
    await gateway.send(DEVICE.id, msg);

    expect(adapter.sentFrames).toHaveLength(1);
    const decoded = new Codec().decode<GatewayToBridgeMsg>(adapter.sentFrames[0].frame);
    expect(decoded.meta).toEqual({ kind: 'event' });
    expect(decoded.data.type).toBe('version');
    await gateway.stop();
  });

  test('reassembles a frame split across multiple chunks', async () => {
    const adapter = new MockAdapter();
    const gateway = new BridgethingGateway(adapter);
    const events = recordEvents(gateway);
    await gateway.start();

    adapter.emit({ type: 'connected', device: DEVICE });

    const msg: BridgeToGatewayMsg = {
      id: newUuidBytes(),
      meta: { kind: 'event' },
      data: { type: 'ack' },
    };
    const frame = new Codec().encode(msg);

    adapter.emit({ type: 'bytes', deviceId: DEVICE.id, data: frame.subarray(0, 4) });
    adapter.emit({ type: 'bytes', deviceId: DEVICE.id, data: frame.subarray(4, 18) });
    adapter.emit({ type: 'bytes', deviceId: DEVICE.id, data: frame.subarray(18) });

    const message = events.find(e => e.type === 'message');
    expect(message).toBeDefined();
    if (message?.type !== 'message') throw new Error('unreachable');
    expect(message.message.data.type).toBe('ack');
    await gateway.stop();
  });

  test('request resolves on matching response', async () => {
    const adapter = new MockAdapter();
    const gateway = new BridgethingGateway(adapter);
    await gateway.start();
    adapter.emit({ type: 'connected', device: DEVICE });

    const responder = (async () => {
      // Wait for the gateway to push the request.
      while (adapter.sentFrames.length === 0) await Bun.sleep(1);
      const sent = new Codec().decode<GatewayToBridgeMsg>(adapter.sentFrames[0].frame);
      const response: BridgeToGatewayMsg = {
        id: newUuidBytes(),
        meta: { kind: 'response', data: { requestId: sent.id } },
        data: { type: 'ack' },
      };
      const frame = new Codec().encode(response);
      adapter.emit({ type: 'bytes', deviceId: DEVICE.id, data: frame });
    })();

    const reply = await gateway.request(DEVICE.id, { type: 'file', data: { event: 'list' } });
    await responder;

    expect(reply.data.type).toBe('ack');
    if (reply.meta.kind !== 'response') throw new Error('expected response meta');
    await gateway.stop();
  });

  test('request rejects on timeout', async () => {
    const adapter = new MockAdapter();
    const gateway = new BridgethingGateway(adapter);
    await gateway.start();
    adapter.emit({ type: 'connected', device: DEVICE });

    await expect(gateway.request(DEVICE.id, { type: 'file', data: { event: 'list' } }, 25)).rejects.toThrow(
      /timed out/,
    );
    await gateway.stop();
  });

  test('request rejects if shutdown', async () => {
    const adapter = new MockAdapter();
    const gateway = new BridgethingGateway(adapter);
    await gateway.start();
    adapter.emit({ type: 'connected', device: DEVICE });

    const promise = gateway.request(DEVICE.id, { type: 'file', data: { event: 'list' } }, 30_000);
    await gateway.stop();
    await expect(promise).rejects.toThrow(/shutting down/);
  });

  test('emits decodeError on bad frame and resyncs', async () => {
    const adapter = new MockAdapter();
    const gateway = new BridgethingGateway(adapter);
    const events = recordEvents(gateway);
    await gateway.start();
    adapter.emit({ type: 'connected', device: DEVICE });

    // Send 16 bytes with bad magic — accumulator throws on parseFrameHeader.
    adapter.emit({ type: 'bytes', deviceId: DEVICE.id, data: new Uint8Array(16) });
    expect(events.some(e => e.type === 'decodeError')).toBe(true);

    // After resync, a real frame should decode.
    const msg: BridgeToGatewayMsg = {
      id: newUuidBytes(),
      meta: { kind: 'event' },
      data: { type: 'ack' },
    };
    adapter.emit({ type: 'bytes', deviceId: DEVICE.id, data: new Codec().encode(msg) });

    const message = events.find(e => e.type === 'message');
    expect(message).toBeDefined();
    await gateway.stop();
  });
});
