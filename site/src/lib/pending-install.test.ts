import { describe, expect, test } from 'bun:test';
import type { AppEntry, AppVersion } from '@bridgething/catalog';
import { isPlaceholderDownload, parsePendingInstall, toPendingInstall } from './pending-install.ts';

const CALENDAR_ID = '019e6701-13f8-71b5-ba04-85d326630e98';
const SOURCE_A = 'https://apps.bridgething.com/catalog.json';
const REAL_SHA = 'a'.repeat(64);

function ver(overrides: Partial<AppVersion['download']> = {}): AppVersion {
  return {
    version: '0.2.0',
    released_at: '2026-05-31T00:00:00Z',
    download: { url: 'https://apps.bridgething.com/r/x.zip', size: 4096, sha256: REAL_SHA, ...overrides },
    permissions: ['net.fetch'],
    min_libbridgething_version: '0.4.0',
    changelog: null,
  };
}

function app(): AppEntry {
  return {
    id: CALENDAR_ID,
    name: 'Calendar',
    description: 'Events.',
    author: 'JoeyEamigh',
    icon: null,
    homepage: null,
    source: null,
    versions: [ver()],
  };
}

describe('isPlaceholderDownload', () => {
  test('flags rows the publish pipeline has not filled in', () => {
    expect(isPlaceholderDownload({ size: 0, sha256: REAL_SHA })).toBe(true);
    expect(isPlaceholderDownload({ size: 4096, sha256: '0'.repeat(64) })).toBe(true);
    expect(isPlaceholderDownload({ size: 0, sha256: '0'.repeat(64) })).toBe(true);
  });

  test('passes a real published row', () => {
    expect(isPlaceholderDownload({ size: 4096, sha256: REAL_SHA })).toBe(false);
  });
});

describe('toPendingInstall', () => {
  test('carries the source url as provenance', () => {
    const pending = toPendingInstall(app(), ver(), SOURCE_A);
    expect(pending.provenance).toBe(SOURCE_A);
    expect(pending.appId).toBe(CALENDAR_ID);
    expect(pending.version).toBe('0.2.0');
    expect(pending.download.sha256).toBe(REAL_SHA);
    expect(pending.minLibbridgethingVersion).toBe('0.4.0');
  });

  test('round trips through the data attribute', () => {
    const pending = toPendingInstall(app(), ver(), SOURCE_A);
    expect(parsePendingInstall(JSON.stringify(pending))).toEqual(pending);
  });
});

describe('parsePendingInstall', () => {
  test('rejects anything that is not a full intent', () => {
    expect(parsePendingInstall('')).toBeNull();
    expect(parsePendingInstall('not json')).toBeNull();
    expect(parsePendingInstall('null')).toBeNull();
    expect(parsePendingInstall('[]')).toBeNull();
    expect(parsePendingInstall(JSON.stringify({ appId: CALENDAR_ID }))).toBeNull();
    expect(
      parsePendingInstall(
        JSON.stringify({
          appId: CALENDAR_ID,
          name: 'Calendar',
          version: '0.2.0',
          minLibbridgethingVersion: '0.4.0',
          provenance: SOURCE_A,
          download: { url: 'https://x/y.zip', size: 1 },
        }),
      ),
    ).toBeNull();
  });
});
