import type { SessionEvent } from '@bridgething/session-react-native';

import { DEVICE, peer, snapshot } from './fixtures';
import { rig, type Rig } from './harness';

function manifest(channels: Array<{ slug: string; latest: string }>) {
  return {
    updatedAt: '2026-07-01T00:00:00Z',
    channels: channels.map(c => ({
      slug: c.slug,
      name: c.slug,
      stability: 'stable',
      isDefault: c.slug === 'stable',
      latest: c.latest,
      releases: [],
    })),
  };
}

function applied(r: Rig): unknown[][] {
  const calls: unknown[][] = [];
  r.native.__returns.set('applyOtaUpdate', (...args: unknown[]) => {
    calls.push(args);
    return Promise.resolve();
  });
  return calls;
}

describe('installing the latest update', () => {
  test('pushes the channel latest to the device', async () => {
    const r = rig();
    r.native.__returns.set(
      'fetchOtaManifest',
      manifest([{ slug: 'stable', latest: '0.7.0+image.2026.7.1' }]),
    );
    const calls = applied(r);

    await r.ota.installLatestOta(DEVICE, 'stable');

    expect(calls).toEqual([[DEVICE, 'stable', '0.7.0+image.2026.7.1', null]]);
  });

  test('says so when the channel is not in the manifest', async () => {
    const r = rig();
    r.native.__returns.set(
      'fetchOtaManifest',
      manifest([{ slug: 'stable', latest: '0.7.0+image.2026.7.1' }]),
    );
    const calls = applied(r);

    await expect(r.ota.installLatestOta(DEVICE, 'dev')).rejects.toThrow(/dev/);
    expect(calls).toEqual([]);
  });

  test('says so when the channel has no release yet', async () => {
    const r = rig();
    r.native.__returns.set(
      'fetchOtaManifest',
      manifest([{ slug: 'dev', latest: '' }]),
    );
    const calls = applied(r);

    await expect(r.ota.installLatestOta(DEVICE, 'dev')).rejects.toThrow();
    expect(calls).toEqual([]);
  });
});

describe('event fan-out', () => {
  test('a domain registered twice under one name only applies once', () => {
    const r = rig();
    const seen: SessionEvent[] = [];
    const domain = {
      name: 'counter',
      apply: (e: SessionEvent) => seen.push(e),
      reconcile: () => {},
    };
    r.bridge.registerDomain(domain);
    r.bridge.registerDomain({ ...domain });

    r.emit('peerConnected', peer());

    expect(seen).toHaveLength(1);
  });

  test('starting the bridge twice does not double every event', () => {
    const r = rig();
    const seen: SessionEvent[] = [];
    r.bridge.registerDomain({
      name: 'counter',
      apply: (e: SessionEvent) => seen.push(e),
      reconcile: () => {},
    });
    r.bridge.startBridge();

    r.emit('peerConnected', peer());

    expect(seen).toHaveLength(1);
    expect(r.session.useSessionStore.getState().peers).toHaveLength(1);
  });

  test('a resume is reconciled, not replayed as an event', () => {
    const r = rig();
    const events: SessionEvent[] = [];
    const snapshots: unknown[] = [];
    r.bridge.registerDomain({
      name: 'counter',
      apply: (e: SessionEvent) => events.push(e),
      reconcile: s => snapshots.push(s),
    });

    r.emit('resumed', snapshot([]));

    expect(events).toHaveLength(0);
    expect(snapshots).toHaveLength(1);
  });

  test('a domain that throws does not stop the others from updating', () => {
    const r = rig();
    r.bridge.registerDomain({
      name: 'exploder',
      apply: () => {
        throw new Error('cannot parse this frame');
      },
      reconcile: () => {
        throw new Error('cannot parse this snapshot');
      },
    });
    const reached: string[] = [];
    r.bridge.registerDomain({
      name: 'downstream',
      apply: e => reached.push(e.type),
      reconcile: () => reached.push('reconcile'),
    });

    r.emit('peerConnected', peer());
    r.emit('resumed', snapshot([peer()]));

    expect(reached).toEqual(['peerConnected', 'reconcile']);
    expect(r.session.useSessionStore.getState().ledger[DEVICE]).toBeDefined();
  });
});
