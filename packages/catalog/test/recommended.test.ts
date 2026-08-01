import { describe, expect, test } from 'bun:test';
import { recommendedSources } from '../src/resolve.ts';
import type { Catalog, RecommendedSource, SourceCatalog } from '../src/types.ts';

const OFFICIAL = 'https://apps.bridgething.com/catalog.json';
const COMMUNITY = 'https://community.example.com/catalog.json';
const OTHER = 'https://other.example.com/catalog.json';

function recommended(name: string, url: string, attested: boolean): RecommendedSource {
  return { name, url, description: null, attested };
}

function catalog(sources: RecommendedSource[]): Catalog {
  return {
    schema: 'catalog.v1',
    updated_at: '2026-07-25T00:00:00Z',
    repo: { name: 'test', description: 'test', homepage: null, icon: null },
    apps: [],
    recommended_sources: sources,
  };
}

function source(url: string, sources: RecommendedSource[]): SourceCatalog {
  return { url, catalog: catalog(sources) };
}

describe('recommendedSources', () => {
  test('the directory and each subscribed catalog both contribute', () => {
    const quickAdds = recommendedSources({
      directory: catalog([recommended('community', COMMUNITY, false)]),
      orderedCatalogs: [source(OFFICIAL, [recommended('other', OTHER, false)])],
      subscribed: [OFFICIAL],
    });

    expect(quickAdds.map(s => s.url)).toEqual([COMMUNITY, OTHER]);
  });

  test('already-subscribed sources are not offered again', () => {
    const quickAdds = recommendedSources({
      directory: catalog([recommended('community', COMMUNITY, false), recommended('other', OTHER, false)]),
      orderedCatalogs: [],
      subscribed: [OFFICIAL, COMMUNITY],
    });

    expect(quickAdds.map(s => s.url)).toEqual([OTHER]);
  });

  test('the directory decides attestation when a catalog claims the same url', () => {
    const quickAdds = recommendedSources({
      directory: catalog([recommended('community', COMMUNITY, true)]),
      orderedCatalogs: [source(OFFICIAL, [recommended('community', COMMUNITY, false)])],
      subscribed: [],
    });

    expect(quickAdds).toHaveLength(1);
    expect(quickAdds[0]!.attested).toBe(true);
  });

  test('a subscribed catalog cannot mint the attested badge for a url the directory never listed', () => {
    const quickAdds = recommendedSources({
      directory: catalog([]),
      orderedCatalogs: [source(COMMUNITY, [recommended('other', OTHER, true)])],
      subscribed: [COMMUNITY],
    });

    expect(quickAdds).toHaveLength(1);
    expect(quickAdds[0]!.url).toBe(OTHER);
    expect(quickAdds[0]!.attested).toBe(false);
  });

  test('catalog recommendations still surface with no directory feed, never attested', () => {
    const quickAdds = recommendedSources({
      directory: null,
      orderedCatalogs: [source(OFFICIAL, [recommended('other', OTHER, true)])],
      subscribed: [OFFICIAL],
    });

    expect(quickAdds.map(s => s.url)).toEqual([OTHER]);
    expect(quickAdds[0]!.attested).toBe(false);
  });

  test('attested sort ahead of listed, then by name', () => {
    const quickAdds = recommendedSources({
      directory: catalog([
        recommended('zed', 'https://z.example/c.json', false),
        recommended('yew', 'https://y.example/c.json', true),
        recommended('abel', 'https://a.example/c.json', false),
      ]),
      orderedCatalogs: [],
      subscribed: [],
    });

    expect(quickAdds.map(s => s.name)).toEqual(['yew', 'abel', 'zed']);
  });

  test('a source that vouches for nothing contributes nothing', () => {
    expect(recommendedSources({ directory: null, orderedCatalogs: [source(OFFICIAL, [])], subscribed: [] })).toEqual(
      [],
    );
  });
});
