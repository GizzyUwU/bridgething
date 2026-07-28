import { rig } from './harness';

const PEER = { id: 'aa:bb', name: 'Car Thing', status: 'connected' as const };

describe('rig', () => {
  test('a native peerConnected reaches the real store through the real wrapper', () => {
    const r = rig();
    r.emit('peerConnected', PEER);

    expect(r.session.useSessionStore.getState().peers).toHaveLength(1);
    expect(r.session.useSessionStore.getState().ledger['aa:bb']?.lastName).toBe(
      'Car Thing',
    );
  });

  test('mmkv contents survive a relaunch', () => {
    const first = rig();
    first.emit('peerConnected', PEER);

    const second = first.relaunch();
    expect(
      second.session.useSessionStore.getState().ledger['aa:bb'],
    ).toBeDefined();
    expect(second.session.useSessionStore.getState().peers).toHaveLength(0);
  });
});
