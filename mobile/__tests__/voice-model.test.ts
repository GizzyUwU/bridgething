import { snapshot } from './fixtures';
import { rig } from './harness';

describe('voice model delivery state', () => {
  test('progress from native replaces the previous tick', () => {
    const r = rig();

    r.emit('voiceModelStateChanged', {
      status: 'downloading',
      receivedBytes: 1_000_000,
      totalBytes: 127_000_000,
    });
    r.emit('voiceModelStateChanged', {
      status: 'downloading',
      receivedBytes: 64_000_000,
      totalBytes: 127_000_000,
    });

    expect(r.session.useSessionStore.getState().voiceModel).toEqual({
      status: 'downloading',
      receivedBytes: 64_000_000,
      totalBytes: 127_000_000,
    });
  });

  test('a foreground snapshot wins over a download that finished while backgrounded', () => {
    const r = rig();
    r.emit('voiceModelStateChanged', {
      status: 'downloading',
      receivedBytes: 10,
      totalBytes: 127_000_000,
    });

    r.emit('resumed', {
      ...snapshot([]),
      voiceModel: {
        status: 'ready',
        receivedBytes: 0,
        totalBytes: 0,
        version: '0.3.2',
      },
    });

    const state = r.session.useSessionStore.getState().voiceModel;
    expect(state.status).toBe('ready');
    expect(state.version).toBe('0.3.2');
  });

  test('a failure carries its reason so the row can explain the retry', () => {
    const r = rig();

    r.emit('voiceModelStateChanged', {
      status: 'failed',
      receivedBytes: 0,
      totalBytes: 0,
      error: 'the nlu manifest carries no bundle for this platform',
    });

    const state = r.session.useSessionStore.getState().voiceModel;
    expect(state.status).toBe('failed');
    expect(state.error).toContain('no bundle for this platform');
  });

  test('an explicit download asks native without touching the capability', async () => {
    const r = rig();

    await r.session.downloadVoiceModel();

    expect(r.native.__calls).toContain('downloadVoiceModel');
    expect(r.native.__calls).not.toContain('setCapabilityFlags');
  });

  test('the voice understanding switch reaches native both ways', async () => {
    const r = rig();
    const flags = r.session.useSessionStore.getState().capabilityFlags;

    await r.session.updateCapabilityFlags({ ...flags, voiceModel: false });
    await r.session.updateCapabilityFlags({ ...flags, voiceModel: true });

    expect(
      r.native.__calls.filter(c => c === 'setCapabilityFlags'),
    ).toHaveLength(2);
    expect(
      r.session.useSessionStore.getState().capabilityFlags.voiceModel,
    ).toBe(true);
  });
});
