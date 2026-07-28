import type { Catalog } from '@bridgething/catalog';

import { rig, type Rig } from './harness';

const OFFICIAL = 'https://apps.bridgething.com/catalog.json';
const DIRECTORY = 'https://bridgething.com/api/sources.json';
const THIRD_PARTY = 'https://example.test/catalog.json';

function catalog(name: string): Catalog {
  return {
    schema: 'catalog.v1',
    updated_at: '2026-05-31T00:00:00Z',
    repo: { name, description: name, homepage: null, icon: null },
    apps: [],
    recommended_sources: [],
  };
}

function serve(served: Record<string, Catalog>): jest.Mock {
  const fetchMock = jest.fn((url: string) => {
    const body = served[url];
    if (!body)
      return Promise.resolve({ ok: false, status: 503, json: () => ({}) });
    return Promise.resolve({ ok: true, status: 200, json: () => body });
  });
  globalThis.fetch = fetchMock as unknown as typeof fetch;
  return fetchMock;
}

const sourcesOf = (r: Rig) => r.catalog.useCatalogStore.getState().sources;

describe('catalog sources', () => {
  test('a source that fails does not take the working ones down with it', async () => {
    const r = rig();
    serve({ [OFFICIAL]: catalog('official'), [DIRECTORY]: catalog('dir') });
    await r.catalog.addSource(THIRD_PARTY);

    const state = r.catalog.useCatalogStore.getState();
    expect(state.catalogs.map(c => c.url)).toEqual([OFFICIAL]);
    expect(state.failures.map(f => f.url)).toEqual([THIRD_PARTY]);
    expect(state.refreshing).toBe(false);
  });

  test('the source directory is not offered as a catalog of apps', async () => {
    const r = rig();
    serve({ [OFFICIAL]: catalog('official'), [DIRECTORY]: catalog('dir') });
    await r.catalog.refreshCatalog();

    const state = r.catalog.useCatalogStore.getState();
    expect(state.catalogs.map(c => c.url)).not.toContain(DIRECTORY);
    expect(state.directory).not.toBeNull();
  });

  test('subscriptions survive an app relaunch', async () => {
    const first = rig();
    serve({
      [OFFICIAL]: catalog('official'),
      [DIRECTORY]: catalog('dir'),
      [THIRD_PARTY]: catalog('third'),
    });
    await first.catalog.addSource(THIRD_PARTY);

    expect(sourcesOf(first.relaunch())).toContain(THIRD_PARTY);
  });

  test('unsubscribing drops the source and its apps', async () => {
    const r = rig();
    serve({
      [OFFICIAL]: catalog('official'),
      [DIRECTORY]: catalog('dir'),
      [THIRD_PARTY]: catalog('third'),
    });
    await r.catalog.addSource(THIRD_PARTY);
    await r.catalog.removeSource(THIRD_PARTY);

    expect(sourcesOf(r)).not.toContain(THIRD_PARTY);
    expect(
      r.catalog.useCatalogStore.getState().catalogs.map(c => c.url),
    ).not.toContain(THIRD_PARTY);
    expect(sourcesOf(r.relaunch())).not.toContain(THIRD_PARTY);
  });

  test('unsubscribing wins over a refresh that was already in flight', async () => {
    const r = rig();
    let releaseThirdParty = () => {};
    const held = new Promise<void>(resolve => {
      releaseThirdParty = resolve;
    });

    globalThis.fetch = jest.fn(async (url: string) => {
      if (url === THIRD_PARTY) await held;
      const body: Record<string, Catalog> = {
        [OFFICIAL]: catalog('official'),
        [DIRECTORY]: catalog('dir'),
        [THIRD_PARTY]: catalog('third'),
      };
      return { ok: true, status: 200, json: () => body[url] };
    }) as unknown as typeof fetch;

    const adding = r.catalog.addSource(THIRD_PARTY);
    await r.catalog.removeSource(THIRD_PARTY);
    releaseThirdParty();
    await adding;

    expect(sourcesOf(r)).not.toContain(THIRD_PARTY);
    expect(
      r.catalog.useCatalogStore.getState().catalogs.map(c => c.url),
    ).not.toContain(THIRD_PARTY);
  });

  test('a corrupt subscription list falls back to the official catalog', () => {
    const r = rig();
    r.storage.storage.set('catalog.sources', 'not json at all');

    expect(sourcesOf(r.relaunch())).toEqual([OFFICIAL]);
  });

  test('an empty subscription list falls back to the official catalog', () => {
    const r = rig();
    r.storage.storage.set('catalog.sources', '[]');

    expect(sourcesOf(r.relaunch())).toEqual([OFFICIAL]);
  });
});
