import { describe, expect, test } from 'bun:test';
import { aggregate, type InstalledWebapp, newestCompatible, pinsFrom, satisfies, updates } from '../src/resolve.ts';
import type { AppEntry, AppVersion, Catalog } from '../src/types.ts';

const CALENDAR_ID = '019e6701-13f8-71b5-ba04-85d326630e98';
const WEATHER_ID = '019e6701-13f8-71b5-ba04-81f347137de2';
const SOURCE_A = 'https://apps.bridgething.com/catalog.json';
const SOURCE_B = 'https://repo.example.com/catalog.json';

function ver(version: string, opts: { minLib?: string; released?: string } = {}): AppVersion {
  return {
    version,
    released_at: opts.released ?? '2026-05-31T00:00:00Z',
    download: { url: `https://apps.bridgething.com/r/${version}.zip`, size: 1, sha256: '0'.repeat(64) },
    permissions: ['net.fetch'],
    min_libbridgething_version: opts.minLib ?? '0.4.0',
    changelog: null,
  };
}

function app(id: string, name: string, versions: AppVersion[]): AppEntry {
  return {
    id,
    name,
    description: 'test',
    author: 'JoeyEamigh',
    icon: null,
    homepage: null,
    source: null,
    versions,
  };
}

function catalog(apps: AppEntry[]): Catalog {
  return {
    schema: 'catalog.v1',
    updated_at: '2026-05-31T00:00:00Z',
    repo: { name: 'test', description: 'test', homepage: null, icon: null },
    apps,
    recommended_sources: [],
  };
}

function installed(
  id: string,
  version: string,
  opts: { source?: 'builtin' | 'installed'; role?: 'standard' | 'launcher'; provenance?: string } = {},
): InstalledWebapp {
  return {
    id,
    version,
    source: opts.source ?? 'installed',
    role: opts.role ?? 'standard',
    provenance: opts.provenance ?? null,
  };
}

function orderedCatalogs() {
  const a = catalog([app(CALENDAR_ID, 'Calendar', [ver('0.2.0'), ver('0.1.0', { released: '2026-04-01T00:00:00Z' })])]);
  const b = catalog([
    app(CALENDAR_ID, 'Calendar', [
      ver('0.3.0', { minLib: '99.0.0' }),
      ver('0.1.5', { released: '2026-04-15T00:00:00Z' }),
    ]),
    app(WEATHER_ID, 'Weather', [ver('0.1.0')]),
  ]);
  return [
    { url: SOURCE_A, catalog: a },
    { url: SOURCE_B, catalog: b },
  ];
}

describe('semver compat', () => {
  test('strips prefix and suffix', () => {
    expect(satisfies('v0.4.1', '0.4.0')).toBe(true);
    expect(satisfies('0.4.0', '0.4.0')).toBe(true);
    expect(satisfies('v0.3.9', '0.4.0')).toBe(false);
    expect(satisfies('v0.5.0-dev', '0.4.0')).toBe(true);
    expect(satisfies('v2.0.0', '2')).toBe(true);
  });
});

describe('provenance', () => {
  test('pins come from device reported provenance', () => {
    const pins = pinsFrom([installed(CALENDAR_ID, '0.1.0', { provenance: SOURCE_B }), installed(WEATHER_ID, '0.1.0')]);
    expect(pins.get(CALENDAR_ID)).toBe(SOURCE_B);
    expect(pins.get(WEATHER_ID)).toBeUndefined();
  });

  test('unrecognized provenance never resolves to a subscribed source', () => {
    const pins = pinsFrom([installed(CALENDAR_ID, '0.1.0', { provenance: 'not a url at all' })]);
    expect(pins.get(CALENDAR_ID)).not.toBe(SOURCE_A);
    expect(pins.get(CALENDAR_ID)).not.toBe(SOURCE_B);
  });

  test('a device that predates provenance degrades to first subscribed source', () => {
    const listings = aggregate({
      orderedCatalogs: orderedCatalogs(),
      installed: [installed(CALENDAR_ID, '0.1.5')],
      deviceLibVersion: 'v0.4.1',
    });
    const cal = listings.find(l => l.app.id === CALENDAR_ID)!;
    expect(cal.sourceUrl).toBe(SOURCE_A);
  });
});

describe('version ordering', () => {
  test('newest is by released_at not array order', () => {
    const a = app(CALENDAR_ID, 'Calendar', [
      ver('0.1.0', { released: '2026-01-01T00:00:00Z' }),
      ver('0.9.0', { released: '2026-06-01T00:00:00Z' }),
    ]);
    expect(newestCompatible(a, 'v0.4.1')?.version).toBe('0.9.0');
  });

  test('non utc offsets compare as instants', () => {
    const a = app(CALENDAR_ID, 'Calendar', [
      ver('0.2.0', { released: '2026-06-01T00:00:00+02:00' }),
      ver('0.1.0', { released: '2026-06-01T00:00:00Z' }),
    ]);
    expect(newestCompatible(a, 'v0.4.1')?.version).toBe('0.1.0');
  });

  test('unparseable timestamps sort last', () => {
    const a = app(CALENDAR_ID, 'Calendar', [
      ver('0.1.0', { released: 'whenever' }),
      ver('0.3.0', { released: '2026-02-01T00:00:00Z' }),
    ]);
    expect(newestCompatible(a, 'v0.4.1')?.version).toBe('0.3.0');
  });

  test('compat filter applies after sorting', () => {
    const a = app(CALENDAR_ID, 'Calendar', [
      ver('0.1.0', { released: '2026-01-01T00:00:00Z' }),
      ver('0.9.0', { minLib: '99.0.0', released: '2026-06-01T00:00:00Z' }),
      ver('0.5.0', { released: '2026-03-01T00:00:00Z' }),
    ]);
    expect(newestCompatible(a, 'v0.4.1')?.version).toBe('0.5.0');
  });
});

describe('aggregate', () => {
  test('pinned source is primary and compat filters', () => {
    const listings = aggregate({
      orderedCatalogs: orderedCatalogs(),
      installed: [installed(CALENDAR_ID, '0.1.5', { provenance: SOURCE_B })],
      deviceLibVersion: 'v0.4.1',
    });
    expect(listings).toHaveLength(2);
    const cal = listings.find(l => l.app.id === CALENDAR_ID)!;
    expect(cal.sourceUrl).toBe(SOURCE_B);
    expect(cal.newestCompatible?.version).toBe('0.1.5');
    expect(cal.installedVersion).toBe('0.1.5');
    expect(cal.updateAvailable).toBe(false);
    expect(cal.alsoAvailableFrom).toEqual([SOURCE_A]);

    const weather = listings.find(l => l.app.id === WEATHER_ID)!;
    expect(weather.installedVersion).toBeNull();
    expect(weather.newestCompatible?.version).toBe('0.1.0');
    expect(weather.alsoAvailableFrom).toEqual([]);
  });

  test('a newer listing than what is installed is an update', () => {
    const a = catalog([app(CALENDAR_ID, 'Calendar', [ver('0.2.0')])]);
    const listings = aggregate({
      orderedCatalogs: [{ url: SOURCE_A, catalog: a }],
      installed: [installed(CALENDAR_ID, '0.1.0', { provenance: SOURCE_A })],
      deviceLibVersion: 'v0.4.1',
    });
    expect(listings[0]!.updateAvailable).toBe(true);
  });

  test('an older listing than what is installed is not an update', () => {
    const a = catalog([app(CALENDAR_ID, 'Calendar', [ver('0.1.0', { released: '2026-06-01T00:00:00Z' })])]);
    const listings = aggregate({
      orderedCatalogs: [{ url: SOURCE_A, catalog: a }],
      installed: [installed(CALENDAR_ID, '0.2.0', { provenance: SOURCE_A })],
      deviceLibVersion: 'v0.4.1',
    });
    expect(listings[0]!.updateAvailable).toBe(false);
  });

  test('defaults to first source when unpinned', () => {
    const listings = aggregate({ orderedCatalogs: orderedCatalogs(), installed: [], deviceLibVersion: 'v0.4.1' });
    const cal = listings.find(l => l.app.id === CALENDAR_ID)!;
    expect(cal.sourceUrl).toBe(SOURCE_A);
    expect(cal.newestCompatible?.version).toBe('0.2.0');
    expect(cal.alsoAvailableFrom).toEqual([SOURCE_B]);
  });

  test('no compatible version for an old device', () => {
    const a = catalog([app(CALENDAR_ID, 'Calendar', [ver('0.3.0', { minLib: '99.0.0' })])]);
    const listings = aggregate({
      orderedCatalogs: [{ url: SOURCE_A, catalog: a }],
      installed: [],
      deviceLibVersion: 'v0.4.1',
    });
    expect(listings[0]!.newestCompatible).toBeNull();
  });

  test('null device version lists newest', () => {
    const a = catalog([app(CALENDAR_ID, 'Calendar', [ver('0.3.0', { minLib: '99.0.0' })])]);
    const listings = aggregate({
      orderedCatalogs: [{ url: SOURCE_A, catalog: a }],
      installed: [],
      deviceLibVersion: null,
    });
    expect(listings[0]!.newestCompatible?.version).toBe('0.3.0');
  });

  test('a dead source never hides an installed app offered by a live one', () => {
    const a = catalog([app(CALENDAR_ID, 'Calendar', [ver('0.2.0')])]);
    const listings = aggregate({
      orderedCatalogs: [{ url: SOURCE_A, catalog: a }],
      installed: [installed(CALENDAR_ID, '0.1.0', { provenance: SOURCE_B })],
      deviceLibVersion: 'v0.4.1',
    });
    const cal = listings.find(l => l.app.id === CALENDAR_ID)!;
    expect(cal.installedVersion).toBe('0.1.0');
    expect(cal.sourceUrl).toBe(SOURCE_A);
  });
});

describe('updates', () => {
  test('offers update only from the pinned source', () => {
    const a = catalog([
      app(CALENDAR_ID, 'Calendar', [ver('0.2.0'), ver('0.1.0', { released: '2026-04-01T00:00:00Z' })]),
    ]);
    const b = catalog([
      app(CALENDAR_ID, 'Calendar', [ver('0.3.0'), ver('0.1.0', { released: '2026-04-01T00:00:00Z' })]),
    ]);
    const catalogs = new Map([
      [SOURCE_A, a],
      [SOURCE_B, b],
    ]);

    const found = updates({
      catalogs,
      installed: [installed(CALENDAR_ID, '0.1.0', { provenance: SOURCE_A })],
      deviceLibVersion: 'v0.4.1',
    });
    expect(found).toHaveLength(1);
    expect(found[0]!.target.version).toBe('0.2.0');
    expect(found[0]!.sourceUrl).toBe(SOURCE_A);
    expect(found[0]!.installedVersion).toBe('0.1.0');
  });

  test('skips unpinned, builtin, and up to date', () => {
    const a = catalog([app(CALENDAR_ID, 'Calendar', [ver('0.2.0')])]);
    const catalogs = new Map([[SOURCE_A, a]]);

    expect(
      updates({ catalogs, installed: [installed(CALENDAR_ID, '0.1.0')], deviceLibVersion: 'v0.4.1' }),
    ).toHaveLength(0);
    expect(
      updates({
        catalogs,
        installed: [installed(CALENDAR_ID, '0.1.0', { source: 'builtin', provenance: SOURCE_A })],
        deviceLibVersion: 'v0.4.1',
      }),
    ).toHaveLength(0);
    expect(
      updates({
        catalogs,
        installed: [installed(CALENDAR_ID, '0.2.0', { provenance: SOURCE_A })],
        deviceLibVersion: 'v0.4.1',
      }),
    ).toHaveLength(0);
  });

  test('a dead pinned source offers nothing but does not throw', () => {
    const catalogs = new Map<string, Catalog>();
    expect(
      updates({
        catalogs,
        installed: [installed(CALENDAR_ID, '0.1.0', { provenance: SOURCE_B })],
        deviceLibVersion: 'v0.4.1',
      }),
    ).toHaveLength(0);
  });

  test('an older version published later is not an update', () => {
    const a = catalog([
      app(CALENDAR_ID, 'Calendar', [
        ver('0.1.0', { released: '2026-06-01T00:00:00Z' }),
        ver('0.2.0', { released: '2026-05-01T00:00:00Z' }),
      ]),
    ]);
    const found = updates({
      catalogs: new Map([[SOURCE_A, a]]),
      installed: [installed(CALENDAR_ID, '0.2.0', { provenance: SOURCE_A })],
      deviceLibVersion: 'v0.4.1',
    });
    expect(found).toHaveLength(0);
  });

  test('matches a catalog id that differs only in case', () => {
    const a = catalog([app(CALENDAR_ID.toUpperCase(), 'Calendar', [ver('0.2.0')])]);
    const found = updates({
      catalogs: new Map([[SOURCE_A, a]]),
      installed: [installed(CALENDAR_ID, '0.1.0', { provenance: SOURCE_A })],
      deviceLibVersion: 'v0.4.1',
    });
    expect(found).toHaveLength(1);
    expect(found[0]!.target.version).toBe('0.2.0');
  });
});
