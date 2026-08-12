import type { AppVersion } from '@bridgething/catalog';

import { describeVersionInstall } from '../lib/catalog';

const BUILD_VOCAB = /\b(daemon|image|channel|wire|adapter|libbridgething)\b/i;

function version(over: Partial<AppVersion> = {}): AppVersion {
  return {
    version: '1.2.0',
    released_at: '2026-05-31T00:00:00Z',
    download: {
      url: 'https://example.test/r/app/1.2.0.zip',
      size: 4096,
      sha256: 'a'.repeat(64),
    },
    permissions: [],
    min_libbridgething_version: '0.1.0',
    changelog: null,
    ...over,
  };
}

describe('the version install question', () => {
  test('names the version it would put on the car thing', () => {
    const copy = describeVersionInstall({
      version: version(),
      newest: version({ version: '2.0.0' }),
      installedVersion: null,
    });

    expect(copy.title).toContain('1.2.0');
    expect(copy.body).not.toMatch(BUILD_VOCAB);
  });

  test('says which version is being replaced when it is a step back', () => {
    const copy = describeVersionInstall({
      version: version(),
      newest: version({ version: '2.0.0' }),
      installedVersion: '2.0.0',
    });

    expect(copy.body).toContain('1.2.0');
    expect(copy.body).toContain('2.0.0');
  });

  test('a first install states the replacement without naming a predecessor', () => {
    const copy = describeVersionInstall({
      version: version(),
      newest: version({ version: '2.0.0' }),
      installedVersion: null,
    });

    expect(copy.body).not.toContain('2.0.0');
  });

  test('anything short of the newest build warns that an update undoes it', () => {
    const copy = describeVersionInstall({
      version: version(),
      newest: version({ version: '2.0.0' }),
      installedVersion: null,
    });

    expect(copy.warning).toContain('2.0.0');
    expect(copy.warning).toMatch(/newest/);
  });

  test('picking the newest build carries no warning', () => {
    const newest = version({ version: '2.0.0' });

    expect(
      describeVersionInstall({
        version: newest,
        newest,
        installedVersion: '1.2.0',
      }).warning,
    ).toBeNull();
  });

  test('a device with no compatible build to compare against carries no warning', () => {
    expect(
      describeVersionInstall({
        version: version(),
        newest: null,
        installedVersion: null,
      }).warning,
    ).toBeNull();
  });

  test('the size and firmware floor live on a labelled detail line', () => {
    const copy = describeVersionInstall({
      version: version({ min_libbridgething_version: '0.4.0' }),
      newest: null,
      installedVersion: null,
    });

    expect(copy.detail.startsWith('detail:')).toBe(true);
    expect(copy.detail).toContain('0.4.0');
    expect(copy.detail).toContain('4 KB');
    expect(copy.body).not.toContain('0.4.0');
  });
});
