import { afterEach, describe, expect, test } from 'bun:test';
import { fetchBundle } from './catalog-source.ts';

const URL_ = 'https://apps.bridgething.com/r/x.zip';
const realFetch = globalThis.fetch;

function stub(respond: () => Response): void {
  globalThis.fetch = (async () => respond()) as unknown as typeof fetch;
}

function buffer(values: number[]): ArrayBuffer {
  const buf = new ArrayBuffer(values.length);
  new Uint8Array(buf).set(values);
  return buf;
}

async function sha256Of(buf: ArrayBuffer): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', buf);
  return Array.from(new Uint8Array(digest))
    .map(b => b.toString(16).padStart(2, '0'))
    .join('');
}

afterEach(() => {
  globalThis.fetch = realFetch;
});

describe('fetchBundle', () => {
  test('returns the blob when size and sha256 both match', async () => {
    const buf = buffer([1, 2, 3, 4]);
    stub(() => new Response(buf, { status: 200 }));
    const result = await fetchBundle({ url: URL_, size: 4, sha256: await sha256Of(buf) });
    expect(result.ok).toBe(true);
    if (result.ok) expect(result.blob.size).toBe(4);
  });

  test('accepts an uppercase digest from the catalog', async () => {
    const buf = buffer([9, 9]);
    stub(() => new Response(buf, { status: 200 }));
    const sha = (await sha256Of(buf)).toUpperCase();
    expect((await fetchBundle({ url: URL_, size: 2, sha256: sha })).ok).toBe(true);
  });

  test('refuses bytes whose digest does not match', async () => {
    stub(() => new Response(buffer([1, 2, 3, 4]), { status: 200 }));
    const result = await fetchBundle({ url: URL_, size: 4, sha256: 'b'.repeat(64) });
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.kind).toBe('integrity');
      expect(result.message).toContain('refusing to install it');
    }
  });

  test('refuses a size mismatch before hashing', async () => {
    stub(() => new Response(buffer([1, 2, 3]), { status: 200 }));
    const result = await fetchBundle({ url: URL_, size: 99, sha256: 'a'.repeat(64) });
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.kind).toBe('integrity');
      expect(result.message).toContain('99');
    }
  });

  test('surfaces an http status', async () => {
    stub(() => new Response('nope', { status: 404, statusText: 'Not Found' }));
    const result = await fetchBundle({ url: URL_, size: 4, sha256: 'a'.repeat(64) });
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.kind).toBe('http');
  });

  test('names the CORS header when the request is blocked outright', async () => {
    stub(() => {
      throw new TypeError('Failed to fetch');
    });
    const result = await fetchBundle({ url: URL_, size: 4, sha256: 'a'.repeat(64) });
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.kind).toBe('blocked');
      expect(result.message).toContain('Access-Control-Allow-Origin');
    }
  });
});
