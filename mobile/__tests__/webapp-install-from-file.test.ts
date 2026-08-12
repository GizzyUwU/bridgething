import { DEVICE } from './fixtures';
import { rig, type Rig } from './harness';

function installedAs(r: Rig, name: string) {
  const calls: unknown[][] = [];
  r.native.__returns.set('installWebapp', (...args: unknown[]) => {
    calls.push(args);
    return Promise.resolve({
      id: 'app',
      name,
      version: '1.0.0',
      source: 'installed',
      role: 'standard',
    });
  });
  return calls;
}

describe('installing a webapp zip picked from files', () => {
  test('the archive reaches native as a local file uri', async () => {
    const r = rig();
    const calls = installedAs(r, 'my webapp');

    const info = await r.webapps.installPickedWebapp(DEVICE);

    expect(calls).toEqual([[DEVICE, 'file:///caches/my-webapp.zip']]);
    expect(info?.name).toBe('my webapp');
  });

  test('the picker is asked for a zip, not for anything on the device', async () => {
    const r = rig();
    installedAs(r, 'my webapp');

    await r.webapps.installPickedWebapp(DEVICE);

    expect(r.picker.fakePicker().pickOptions).toEqual([
      { type: [r.picker.types.zip], mode: 'import' },
    ]);
  });

  test('an android content uri is copied out before native ever sees it', async () => {
    const r = rig({ platform: 'android' });
    const picker = r.picker.fakePicker();
    picker.picked = {
      uri: 'content://com.android.providers.downloads/document/42',
      name: 'my-webapp.zip',
    };
    const calls = installedAs(r, 'my webapp');

    await r.webapps.installPickedWebapp(DEVICE);

    expect(picker.copies).toEqual([
      {
        uri: 'content://com.android.providers.downloads/document/42',
        fileName: 'my-webapp.zip',
        destination: 'cachesDirectory',
      },
    ]);
    expect(calls).toEqual([[DEVICE, 'file:///caches/my-webapp.zip']]);
  });

  test('backing out of the picker installs nothing and is not an error', async () => {
    const r = rig();
    r.picker.fakePicker().picked = null;
    const calls = installedAs(r, 'my webapp');

    await expect(r.webapps.installPickedWebapp(DEVICE)).resolves.toBeNull();
    expect(calls).toEqual([]);
    expect(r.native.__calls).not.toContain('installWebapp');
  });

  test('a file the picker cannot copy out is reported, not installed', async () => {
    const r = rig({ platform: 'android' });
    r.picker.fakePicker().copyError = 'permission denied';
    const calls = installedAs(r, 'my webapp');

    await expect(r.webapps.installPickedWebapp(DEVICE)).rejects.toThrow(
      /my-webapp\.zip.*permission denied/,
    );
    expect(calls).toEqual([]);
  });

  test('a picked file with no name still installs under a fallback name', async () => {
    const r = rig();
    r.picker.fakePicker().picked = {
      uri: 'file:///var/inbox/unnamed',
      name: null,
    };
    const calls = installedAs(r, 'my webapp');

    await r.webapps.installPickedWebapp(DEVICE);

    expect(calls).toEqual([[DEVICE, 'file:///caches/webapp.zip']]);
  });
});
