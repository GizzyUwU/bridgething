import { afterEach, describe, expect, test } from 'bun:test';

import { ICON_CACHE_LIMIT, cacheIcon, cachedIcon, fetchIcon, looksLikeSvg, svgDataUrl } from './icon.ts';

const SVG = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M0 0h16v16H0z"/></svg>';

const real = globalThis.fetch;

function stub(handler: () => Promise<Response>): void {
  globalThis.fetch = handler as unknown as typeof fetch;
}

function answer(body: string, headers: Record<string, string>, status = 200): void {
  stub(() => Promise.resolve(new Response(body, { status, headers })));
}

afterEach(() => {
  globalThis.fetch = real;
});

describe('looksLikeSvg', () => {
  test('trusts an svg content type without reading the body', () => {
    expect(looksLikeSvg('image/svg+xml', '')).toBe(true);
    expect(looksLikeSvg('image/svg+xml; charset=utf-8', '')).toBe(true);
  });

  test('sniffs the body when the server would not say', () => {
    expect(looksLikeSvg(null, SVG)).toBe(true);
    expect(looksLikeSvg('application/octet-stream', SVG)).toBe(true);
  });

  test('sees past an xml prolog and leading comments', () => {
    expect(looksLikeSvg(null, `<?xml version="1.0"?>\n<!-- built by hand -->\n${SVG}`)).toBe(true);
  });

  test('rejects markup that only starts like svg', () => {
    expect(looksLikeSvg(null, '<svgsomething />')).toBe(false);
    expect(looksLikeSvg(null, '<html><body><svg /></body></html>')).toBe(false);
  });

  test('rejects raster bytes read as text', () => {
    expect(looksLikeSvg('image/png', '\x89PNG\r\n\x1a\n')).toBe(false);
    expect(looksLikeSvg(null, '\x89PNG\r\n\x1a\n')).toBe(false);
  });
});

describe('svgDataUrl', () => {
  test('escapes the markup so it survives an img src', () => {
    const url = svgDataUrl('<svg><path d="M0 0h1v1H0z"/></svg>');
    expect(url.startsWith('data:image/svg+xml;utf8,')).toBe(true);
    expect(url).not.toContain('<');
    expect(decodeURIComponent(url.slice('data:image/svg+xml;utf8,'.length))).toBe('<svg><path d="M0 0h1v1H0z"/></svg>');
  });
});

describe('fetchIcon', () => {
  test('a declared raster type stays a url, so the img refetches from cache', async () => {
    answer('binary', { 'content-type': 'image/png' });
    expect(await fetchIcon('https://example.test/a.png')).toEqual({
      kind: 'raster',
      url: 'https://example.test/a.png',
    });
  });

  test('a declared svg type comes back as markup', async () => {
    answer(SVG, { 'content-type': 'image/svg+xml' });
    expect(await fetchIcon('https://example.test/a.svg')).toEqual({ kind: 'svg', svg: SVG });
  });

  test('an undecided type is settled by sniffing the body', async () => {
    answer(SVG, { 'content-type': 'application/octet-stream' });
    expect(await fetchIcon('https://example.test/a')).toEqual({ kind: 'svg', svg: SVG });

    answer('not markup', { 'content-type': 'application/octet-stream' });
    expect(await fetchIcon('https://example.test/b')).toEqual({ kind: 'raster', url: 'https://example.test/b' });
  });

  test('an error response fails rather than pointing an img at it', async () => {
    answer('nope', { 'content-type': 'image/png' }, 404);
    expect(await fetchIcon('https://example.test/missing.png')).toEqual({ kind: 'failed' });
  });

  test('a declared size over the cap fails before the body is read', async () => {
    answer(SVG, { 'content-type': 'image/svg+xml', 'content-length': String(64 * 1024 + 1) });
    expect(await fetchIcon('https://example.test/huge.svg')).toEqual({ kind: 'failed' });
  });

  test('a body over the cap fails even when the server declared nothing', async () => {
    answer('<svg>'.padEnd(64 * 1024 + 1, 'x'), { 'content-type': 'image/svg+xml' });
    expect(await fetchIcon('https://example.test/lying.svg')).toEqual({ kind: 'failed' });
  });

  test('a transport failure fails rather than throwing at the caller', async () => {
    stub(() => Promise.reject(new Error('offline')));
    expect(await fetchIcon('https://example.test/a.svg')).toEqual({ kind: 'failed' });
  });
});

describe('icon cache', () => {
  test('round-trips a resolution and misses on an unknown key', () => {
    cacheIcon('round-trip', { kind: 'svg', svg: SVG });
    expect(cachedIcon('round-trip')).toEqual({ kind: 'svg', svg: SVG });
    expect(cachedIcon('never-stored')).toBeUndefined();
  });

  test('caches a failure so a broken icon is not refetched on every mount', () => {
    cacheIcon('broken', { kind: 'failed' });
    expect(cachedIcon('broken')).toEqual({ kind: 'failed' });
  });

  test('evicts least-recently-used once past the limit, keeping what was touched', () => {
    for (let i = 0; i < ICON_CACHE_LIMIT; i += 1) cacheIcon(`evict-${i}`, { kind: 'raster', url: `${i}` });

    expect(cachedIcon('evict-0')).toBeDefined();

    for (let i = 0; i < 8; i += 1) cacheIcon(`overflow-${i}`, { kind: 'failed' });

    expect(cachedIcon('evict-0')).toBeDefined();
    expect(cachedIcon('evict-1')).toBeUndefined();
    expect(cachedIcon('overflow-7')).toBeDefined();
  });
});
