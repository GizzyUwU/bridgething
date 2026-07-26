import { describe, expect, test } from 'bun:test';
import { recommendedSources } from '../src/resolve.ts';
import type { Catalog, RecommendedSource, SourceCatalog } from '../src/types.ts';

const DIRECTORY = 'https://bridgething.com/api/sources.json';
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
      orderedCatalogs: [
        source(DIRECTORY, [recommended('community', COMMUNITY, false)]),
        source(OFFICIAL, [recommended('other', OTHER, true)]),
      ],
      subscribed: [OFFICIAL],
    });

    expect(quickAdds.map(s => s.url)).toEqual([OTHER, COMMUNITY]);
  });

  test('already-subscribed sources are not offered again', () => {
    const quickAdds = recommendedSources({
      orderedCatalogs: [
        source(DIRECTORY, [recommended('community', COMMUNITY, false), recommended('other', OTHER, false)]),
      ],
      subscribed: [OFFICIAL, COMMUNITY],
    });

    expect(quickAdds.map(s => s.url)).toEqual([OTHER]);
  });

  test('the first catalog to claim a url wins, so the directory decides attestation', () => {
    const quickAdds = recommendedSources({
      orderedCatalogs: [
        source(DIRECTORY, [recommended('community', COMMUNITY, false)]),
        source(OFFICIAL, [recommended('community', COMMUNITY, true)]),
      ],
      subscribed: [],
    });

    expect(quickAdds).toHaveLength(1);
    expect(quickAdds[0]!.attested).toBe(false);
  });

  test('attested sort ahead of listed, then by name', () => {
    const quickAdds = recommendedSources({
      orderedCatalogs: [
        source(DIRECTORY, [
          recommended('zed', 'https://z.example/c.json', false),
          recommended('yew', 'https://y.example/c.json', true),
          recommended('abel', 'https://a.example/c.json', false),
        ]),
      ],
      subscribed: [],
    });

    expect(quickAdds.map(s => s.name)).toEqual(['yew', 'abel', 'zed']);
  });

  test('a source that vouches for nothing contributes nothing', () => {
    expect(recommendedSources({ orderedCatalogs: [source(OFFICIAL, [])], subscribed: [] })).toEqual([]);
  });
});
