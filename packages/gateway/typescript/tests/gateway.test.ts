import { type BridgeToGatewayMsg, Codec, type GatewayMeta, type GatewayToBridgeMsg, newUuid } from '@bridgething/lib';
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
      id: newUuid(),
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
      id: newUuid(),
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
        id: newUuid(),
        meta: { kind: 'response', data: { requestId: sent.id } },
        data: { type: 'ack' },
      };
      const frame = new Codec().encode(response);
      adapter.emit({ type: 'bytes', deviceId: DEVICE.id, data: frame });
    })();

    const reply = await gateway.request(DEVICE.id, { type: 'webapp', data: { event: 'getActive' } });
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

    await expect(gateway.request(DEVICE.id, { type: 'webapp', data: { event: 'getActive' } }, 25)).rejects.toThrow(
      /timed out/,
    );
    await gateway.stop();
  });

  test('request rejects if shutdown', async () => {
    const adapter = new MockAdapter();
    const gateway = new BridgethingGateway(adapter);
    await gateway.start();
    adapter.emit({ type: 'connected', device: DEVICE });

    const promise = gateway.request(DEVICE.id, { type: 'webapp', data: { event: 'getActive' } }, 30_000);
    await gateway.stop();
    await expect(promise).rejects.toThrow(/shutting down/);
  });

  test('asset.sendPush encodes the right wire shape', async () => {
    const adapter = new MockAdapter();
    const gateway = new BridgethingGateway(adapter);
    await gateway.start();
    adapter.emit({ type: 'connected', device: DEVICE });

    await gateway.device(DEVICE.id).asset.push({
      id: 'spotify/track/abc/image',
      bytes: new Uint8Array([1, 2, 3]),
      mime: 'image/jpeg',
      retention: { type: 'lru' },
    });

    expect(adapter.sentFrames).toHaveLength(1);
    const decoded = new Codec().decode<GatewayToBridgeMsg>(adapter.sentFrames[0].frame);
    expect(decoded.meta.kind).toBe('event');
    expect(decoded.data.type).toBe('asset');
    if (decoded.data.type !== 'asset') throw new Error();
    expect(decoded.data.data.event).toBe('push');
    if (decoded.data.data.event !== 'push') throw new Error();
    expect(decoded.data.data.data.id).toBe('spotify/track/abc/image');
    expect(decoded.data.data.data.bytes).toEqual(new Uint8Array([1, 2, 3]));
    await gateway.stop();
  });

  test('player.onPause fires only for matching variant', async () => {
    const adapter = new MockAdapter();
    const gateway = new BridgethingGateway(adapter);
    let pauseCalls = 0;
    let playCalls = 0;
    gateway.player.onPause(() => pauseCalls++);
    gateway.player.onPlay(() => playCalls++);
    await gateway.start();
    adapter.emit({ type: 'connected', device: DEVICE });

    const pause: BridgeToGatewayMsg = {
      id: newUuid(),
      meta: { kind: 'command' },
      data: { type: 'player', data: { event: 'pause' } },
    };
    adapter.emit({ type: 'bytes', deviceId: DEVICE.id, data: new Codec().encode(pause) });
    expect(pauseCalls).toBe(1);
    expect(playCalls).toBe(0);
    await gateway.stop();
  });

  test('outer subscribePartial routes to per-surface handlers', async () => {
    const adapter = new MockAdapter();
    const gateway = new BridgethingGateway(adapter);
    let assetReqCalls = 0;
    let playerPauseCalls = 0;
    gateway.subscribePartial({
      asset: async handle => {
        assetReqCalls++;
        await handle.respondErr({ id: 'x' });
      },
      player: { pause: () => playerPauseCalls++ },
    });
    await gateway.start();
    adapter.emit({ type: 'connected', device: DEVICE });

    const asset: BridgeToGatewayMsg = {
      id: newUuid(),
      meta: { kind: 'request' },
      data: { type: 'asset', data: { event: 'request', data: { id: 'x', requestId: newUuid() } } },
    };
    adapter.emit({ type: 'bytes', deviceId: DEVICE.id, data: new Codec().encode(asset) });

    const player: BridgeToGatewayMsg = {
      id: newUuid(),
      meta: { kind: 'command' },
      data: { type: 'player', data: { event: 'pause' } },
    };
    adapter.emit({ type: 'bytes', deviceId: DEVICE.id, data: new Codec().encode(player) });

    while (adapter.sentFrames.length === 0) await new Promise(r => setTimeout(r, 5));
    expect(assetReqCalls).toBe(1);
    expect(playerPauseCalls).toBe(1);
    await gateway.stop();
  });

  test('webapp.querySwitchTo returns typed ok', async () => {
    const adapter = new MockAdapter();
    const gateway = new BridgethingGateway(adapter);
    await gateway.start();
    adapter.emit({ type: 'connected', device: DEVICE });

    const responder = (async () => {
      while (adapter.sentFrames.length === 0) await new Promise(r => setTimeout(r, 5));
      const sent = new Codec().decode<GatewayToBridgeMsg>(adapter.sentFrames[0].frame);
      const response: BridgeToGatewayMsg = {
        id: newUuid(),
        meta: { kind: 'response', data: { requestId: sent.id } },
        data: { type: 'webapp', data: { event: 'switched', data: { name: 'newapp' } } },
      };
      adapter.emit({ type: 'bytes', deviceId: DEVICE.id, data: new Codec().encode(response) });
    })();

    const result = await gateway.webapp.switchTo(DEVICE.id, { name: 'newapp' });
    await responder;
    expect(result.ok).toBe(true);
    if (result.ok) expect(result.response.name).toBe('newapp');
    await gateway.stop();
  });

  test('webapp.querySwitchTo returns typed domain error', async () => {
    const adapter = new MockAdapter();
    const gateway = new BridgethingGateway(adapter);
    await gateway.start();
    adapter.emit({ type: 'connected', device: DEVICE });

    const responder = (async () => {
      while (adapter.sentFrames.length === 0) await new Promise(r => setTimeout(r, 5));
      const sent = new Codec().decode<GatewayToBridgeMsg>(adapter.sentFrames[0].frame);
      const response: BridgeToGatewayMsg = {
        id: newUuid(),
        meta: { kind: 'response', data: { requestId: sent.id } },
        data: {
          type: 'webapp',
          data: { event: 'webappError', data: { type: 'unknownWebapp', data: { name: 'missing' } } },
        },
      };
      adapter.emit({ type: 'bytes', deviceId: DEVICE.id, data: new Codec().encode(response) });
    })();

    const result = await gateway.webapp.switchTo(DEVICE.id, { name: 'missing' });
    await responder;
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.kind).toBe('domain');
      if (result.kind === 'domain') expect(result.error.type).toBe('unknownWebapp');
    }
    await gateway.stop();
  });

  test('webapp.queryListWebapps takes no payload', async () => {
    const adapter = new MockAdapter();
    const gateway = new BridgethingGateway(adapter);
    await gateway.start();
    adapter.emit({ type: 'connected', device: DEVICE });

    const responder = (async () => {
      while (adapter.sentFrames.length === 0) await new Promise(r => setTimeout(r, 5));
      const sent = new Codec().decode<GatewayToBridgeMsg>(adapter.sentFrames[0].frame);
      expect(sent.data.type).toBe('webapp');
      if (sent.data.type !== 'webapp') throw new Error();
      expect(sent.data.data.event).toBe('list');
      const response: BridgeToGatewayMsg = {
        id: newUuid(),
        meta: { kind: 'response', data: { requestId: sent.id } },
        data: { type: 'webapp', data: { event: 'webapps', data: { webapps: [] } } },
      };
      adapter.emit({ type: 'bytes', deviceId: DEVICE.id, data: new Codec().encode(response) });
    })();

    const result = await gateway.webapp.list(DEVICE.id);
    await responder;
    expect(result.ok).toBe(true);
    if (result.ok) expect(result.response.webapps).toEqual([]);
    await gateway.stop();
  });

  test('asset.onRequest fires with typed handle + payload', async () => {
    const adapter = new MockAdapter();
    const gateway = new BridgethingGateway(adapter);
    gateway.asset.onRequest(async (handle, req) => {
      expect(handle.deviceId).toBe(DEVICE.id);
      expect(req.id).toBe('spotify/track/abc/image');
      await handle.respond({ id: req.id, bytes: new Uint8Array([7, 8]), mime: 'image/jpeg' });
    });
    await gateway.start();
    adapter.emit({ type: 'connected', device: DEVICE });

    const requestId = newUuid();
    const incoming: BridgeToGatewayMsg = {
      id: requestId,
      meta: { kind: 'request' },
      data: {
        type: 'asset',
        data: { event: 'request', data: { id: 'spotify/track/abc/image', requestId: newUuid() } },
      },
    };
    adapter.emit({ type: 'bytes', deviceId: DEVICE.id, data: new Codec().encode(incoming) });
    while (adapter.sentFrames.length === 0) await new Promise(r => setTimeout(r, 5));

    const sent = new Codec().decode<GatewayToBridgeMsg>(adapter.sentFrames[0].frame);
    expect(sent.meta.kind).toBe('response');
    if (sent.meta.kind !== 'response') throw new Error();
    expect(Array.from(sent.meta.data.requestId)).toEqual(Array.from(requestId));
    expect(sent.data.type).toBe('asset');
    if (sent.data.type !== 'asset') throw new Error();
    expect(sent.data.data.event).toBe('got');
    if (sent.data.data.event !== 'got') throw new Error();
    expect(sent.data.data.data.bytes).toEqual(new Uint8Array([7, 8]));
    await gateway.stop();
  });

  test('asset.onRequest respondErr produces typed NotFound', async () => {
    const adapter = new MockAdapter();
    const gateway = new BridgethingGateway(adapter);
    gateway.asset.onRequest(async (handle, req) => {
      await handle.respondErr({ id: req.id });
    });
    await gateway.start();
    adapter.emit({ type: 'connected', device: DEVICE });

    const incoming: BridgeToGatewayMsg = {
      id: newUuid(),
      meta: { kind: 'request' },
      data: {
        type: 'asset',
        data: { event: 'request', data: { id: 'missing', requestId: newUuid() } },
      },
    };
    adapter.emit({ type: 'bytes', deviceId: DEVICE.id, data: new Codec().encode(incoming) });
    while (adapter.sentFrames.length === 0) await new Promise(r => setTimeout(r, 5));

    const sent = new Codec().decode<GatewayToBridgeMsg>(adapter.sentFrames[0].frame);
    if (sent.data.type !== 'asset') throw new Error();
    expect(sent.data.data.event).toBe('notFound');
    if (sent.data.data.event !== 'notFound') throw new Error();
    expect(sent.data.data.data.id).toBe('missing');
    await gateway.stop();
  });

  test('emits decodeError on bad frame and resyncs', async () => {
    const adapter = new MockAdapter();
    const gateway = new BridgethingGateway(adapter);
    const events = recordEvents(gateway);
    await gateway.start();
    adapter.emit({ type: 'connected', device: DEVICE });

    // Send 16 bytes with bad magic - accumulator throws on parseFrameHeader.
    adapter.emit({ type: 'bytes', deviceId: DEVICE.id, data: new Uint8Array(16) });
    expect(events.some(e => e.type === 'decodeError')).toBe(true);

    // After resync, a real frame should decode.
    const msg: BridgeToGatewayMsg = {
      id: newUuid(),
      meta: { kind: 'event' },
      data: { type: 'ack' },
    };
    adapter.emit({ type: 'bytes', deviceId: DEVICE.id, data: new Codec().encode(msg) });

    const message = events.find(e => e.type === 'message');
    expect(message).toBeDefined();
    await gateway.stop();
  });
});
