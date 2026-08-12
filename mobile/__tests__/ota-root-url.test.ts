import { rig } from './harness';

const CUSTOM = 'https://ota.example';

function captured(r: ReturnType<typeof rig>, method: string): unknown[][] {
  const calls: unknown[][] = [];
  r.native.__returns.set(method, (...args: unknown[]) => {
    calls.push(args);
    return Promise.resolve();
  });
  return calls;
}

describe('the update host override', () => {
  test('falls back to the stock host when nothing is held', () => {
    const r = rig();

    expect(r.ota.rootUrlOf(null)).toBe(r.storage.DEFAULT_OTA_ROOT_URL);
    expect(r.ota.rootUrlOf({ intervalSeconds: 3600, autoPush: true })).toBe(
      r.storage.DEFAULT_OTA_ROOT_URL,
    );
  });

  test('a blank held value is not an override', () => {
    const r = rig();

    expect(
      r.ota.rootUrlOf({
        intervalSeconds: 3600,
        autoPush: true,
        rootUrl: '   ',
      }),
    ).toBe(r.storage.DEFAULT_OTA_ROOT_URL);
  });

  test('a held value wins', () => {
    const r = rig();

    expect(
      r.ota.rootUrlOf({
        intervalSeconds: 3600,
        autoPush: true,
        rootUrl: CUSTOM,
      }),
    ).toBe(CUSTOM);
  });

  test('patching one field keeps the override', async () => {
    const r = rig();
    const written = captured(r, 'setOtaPollConfig');
    await r.session.patchOtaPollConfig({ rootUrl: CUSTOM });
    await r.session.patchOtaPollConfig({ autoPush: false });

    const last = written.at(-1)?.[0] as { rootUrl?: string; autoPush: boolean };
    expect(last.rootUrl).toBe(CUSTOM);
    expect(last.autoPush).toBe(false);
  });

  test('clearing the override sends no host at all', async () => {
    const r = rig();
    const written = captured(r, 'setOtaPollConfig');
    await r.session.patchOtaPollConfig({ rootUrl: CUSTOM });
    await r.session.patchOtaPollConfig({ rootUrl: undefined });

    const last = written.at(-1)?.[0] as { rootUrl?: string };
    expect(last.rootUrl).toBeUndefined();
  });

  test('installing the latest asks the host it was given', async () => {
    const r = rig();
    const applied = captured(r, 'applyOtaUpdate');
    const fetched: unknown[][] = [];
    r.native.__returns.set('fetchOtaManifest', (...args: unknown[]) => {
      fetched.push(args);
      return Promise.resolve({
        updatedAt: '2026-07-01T00:00:00Z',
        channels: [
          {
            slug: 'stable',
            name: 'stable',
            stability: 'stable',
            isDefault: true,
            latest: '0.7.0+image.2026.7.1',
            releases: [],
          },
        ],
      });
    });

    await r.ota.installLatestOta('AA:BB:CC:DD:EE:FF', 'stable', CUSTOM);

    expect(fetched.at(-1)?.[0]).toBe(CUSTOM);
    expect(applied.at(-1)?.at(-1)).toBe(CUSTOM);
  });
});
