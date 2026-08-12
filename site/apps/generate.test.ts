import { describe, expect, test } from 'bun:test';
import type { Repo } from '@bridgething/catalog';
import type { AppConfigEntry, CatalogCuration, PublishedState } from './config.ts';
import { generate, mergeApps } from './generate.ts';

const CALENDAR_ID = '019e6701-13f8-71b5-ba04-85d326630e98';
const WEATHER_ID = '019e6701-13f8-71b5-ba04-81f347137de2';
const SHA = 'a'.repeat(64);

const REPO: Repo = { name: 'bridgething apps', description: 'official', homepage: null, icon: null };

function version(v: string, releasedAt: string) {
  return {
    version: v,
    released_at: releasedAt,
    download: { url: `https://apps.bridgething.com/r/${CALENDAR_ID}/${v}.zip`, size: 100, sha256: SHA },
    permissions: ['net.fetch'],
    min_libbridgething_version: '0.4.0',
    changelog: null,
  };
}

function app(overrides: Partial<AppConfigEntry> = {}): AppConfigEntry {
  return {
    slug: 'calendar',
    id: CALENDAR_ID,
    name: 'Calendar',
    description: 'Upcoming events.',
    author: 'JoeyEamigh',
    icon: null,
    homepage: null,
    source: null,
    versions: [version('0.1.0', '2026-05-31T00:00:00Z')],
    ...overrides,
  };
}

function run(apps: AppConfigEntry[]) {
  return generate({ repo: REPO, recommendedSources: [], apps, updatedAt: '2026-07-25T00:00:00Z' });
}

describe('generate()', () => {
  test('builds a catalog entry from a self-contained config, with no source tree', () => {
    const catalog = run([app()]);

    expect(catalog.apps).toHaveLength(1);
    expect(catalog.apps[0]!.id).toBe(CALENDAR_ID);
    expect(catalog.apps[0]!.name).toBe('Calendar');
    expect(catalog.apps[0]!.description).toBe('Upcoming events.');
  });

  test('orders an app versions newest-first', () => {
    const catalog = run([
      app({ versions: [version('0.1.0', '2026-05-01T00:00:00Z'), version('0.2.0', '2026-06-01T00:00:00Z')] }),
    ]);

    expect(catalog.apps[0]!.versions.map(v => v.version)).toEqual(['0.2.0', '0.1.0']);
  });

  test('carries recommended sources through', () => {
    const catalog = generate({
      repo: REPO,
      recommendedSources: [{ name: 'vouched', url: 'https://a.example/c.json', description: null, attested: true }],
      apps: [app()],
      updatedAt: '2026-07-25T00:00:00Z',
    });

    expect(catalog.recommended_sources).toHaveLength(1);
    expect(catalog.recommended_sources[0]!.attested).toBe(true);
  });

  for (const field of ['id', 'name', 'description'] as const) {
    test(`rejects an app missing "${field}", which the publish dispatch fills in`, () => {
      expect(() => run([app({ [field]: '' })])).toThrow(field);
    });
  }

  test('rejects an app with no versions', () => {
    expect(() => run([app({ versions: [] })])).toThrow('no versions');
  });

  test('rejects two apps sharing one uuid, which is a squat', () => {
    expect(() => run([app(), app({ slug: 'weather', name: 'Weather' })])).toThrow();
  });

  test('accepts distinct apps', () => {
    const catalog = run([app(), app({ slug: 'weather', id: WEATHER_ID, name: 'Weather', description: 'Forecast.' })]);
    expect(catalog.apps.map(a => a.name)).toEqual(['Calendar', 'Weather']);
  });
});

const PUBLISHED: PublishedState = {
  recommended_sources: [],
  apps: [
    {
      slug: 'calendar',
      id: CALENDAR_ID,
      name: 'Calendar',
      description: 'Upcoming events.',
      icon: 'https://apps.bridgething.com/icons/calendar.svg',
      versions: [version('0.1.0', '2026-05-31T00:00:00Z')],
    },
  ],
};

const CURATION: CatalogCuration = {
  repo: REPO,
  apps: [{ slug: 'calendar', author: 'JoeyEamigh', homepage: 'https://bridgething.com/apps', source: null }],
};

describe('mergeApps()', () => {
  test('publish state decides which apps and versions exist', () => {
    const merged = mergeApps({ repo: REPO, apps: [] }, PUBLISHED);

    expect(merged.map(a => a.slug)).toEqual(['calendar']);
    expect(merged[0]!.id).toBe(CALENDAR_ID);
    expect(merged[0]!.versions.map(v => v.version)).toEqual(['0.1.0']);
  });

  test('curation supplies attribution', () => {
    const merged = mergeApps(CURATION, PUBLISHED);

    expect(merged[0]!.author).toBe('JoeyEamigh');
    expect(merged[0]!.homepage).toBe('https://bridgething.com/apps');
    expect(merged[0]!.source).toBeNull();
  });

  test('an app nobody has curated still lands, on the defaults', () => {
    const merged = mergeApps({ repo: REPO }, PUBLISHED);

    expect(merged[0]!.author).toBe('JoeyEamigh');
    expect(merged[0]!.homepage).toBe('https://bridgething.com/apps');
    expect(merged[0]!.source).toBe('https://github.com/JoeyEamigh/bridgething');
  });

  test('curation overrides the listing copy the publish payload carried', () => {
    const merged = mergeApps(
      { repo: REPO, apps: [{ slug: 'calendar', name: 'Agenda', description: 'Fixed copy.', icon: null }] },
      PUBLISHED,
    );

    expect(merged[0]!.name).toBe('Agenda');
    expect(merged[0]!.description).toBe('Fixed copy.');
    expect(merged[0]!.icon).toBeNull();
  });

  test('curation cannot invent an app or a version', () => {
    const merged = mergeApps({ repo: REPO, apps: [{ slug: 'weather', author: 'someone' }] }, PUBLISHED);

    expect(merged.map(a => a.slug)).toEqual(['calendar']);
  });
});
