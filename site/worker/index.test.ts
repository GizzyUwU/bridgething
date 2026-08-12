import { beforeEach, describe, expect, test } from 'bun:test';
import { OFFICIAL_CATALOG_URL } from '@bridgething/catalog';
import { listInstalls, toInstallCounts } from './installs.ts';
import { fakeKv, type FakeKv } from './kv-fake.ts';
import type { Env } from './index.ts';

const CALENDAR_ID = '019e6701-13f8-71b5-ba04-85d326630e98';
const CLIENT = '203.0.113.7';

(globalThis as unknown as { caches: unknown }).caches = {
  default: {
    match: () => Promise.resolve(undefined),
    put: () => Promise.resolve(),
    delete: () => Promise.resolve(true),
  },
};

const worker = (await import('./index.ts')).default;

let kv: FakeKv;

function env(): Env {
  return { SOURCES: kv as unknown as KVNamespace, ASSETS: {} as Fetcher, ADMIN_TOKEN: 'unused' };
}

function context(): ExecutionContext {
  return { waitUntil: () => undefined, passThroughOnException: () => undefined } as unknown as ExecutionContext;
}

function post(path: string, body: unknown, client = CLIENT): Promise<Response> {
  const request = new Request(`https://bridgething.com${path}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', 'cf-connecting-ip': client },
    body: JSON.stringify(body),
  });
  return worker.fetch(request, env(), context());
}

function beacon(sourceUrl = OFFICIAL_CATALOG_URL): Record<string, unknown> {
  return { app_id: CALENDAR_ID, source_url: sourceUrl, version: '1.0.0' };
}

beforeEach(() => {
  kv = fakeKv();
});

describe('POST /api/installs', () => {
  test('an install is accepted and answered with the tally it produced', async () => {
    const response = await post('/api/installs', beacon());

    expect(response.status).toBe(202);
    expect(await response.json<{ installs: number }>()).toEqual({ installs: 1 });
  });

  test('a second install of the same app from the same source adds to the tally', async () => {
    await post('/api/installs', beacon());
    const response = await post('/api/installs', beacon());

    expect(await response.json<{ installs: number }>()).toEqual({ installs: 2 });
    expect(toInstallCounts(await listInstalls(kv))).toEqual([
      { app_id: CALENDAR_ID, source_url: OFFICIAL_CATALOG_URL, count: 2 },
    ]);
  });

  test('a source outside the directory is refused', async () => {
    const response = await post('/api/installs', beacon('https://nobody.example/catalog.json'));

    expect(response.status).toBe(404);
    expect(await listInstalls(kv)).toHaveLength(0);
  });

  test('a body that is not json is refused rather than counted', async () => {
    const request = new Request('https://bridgething.com/api/installs', {
      method: 'POST',
      headers: { 'content-type': 'application/json', 'cf-connecting-ip': CLIENT },
      body: 'not json',
    });

    expect((await worker.fetch(request, env(), context())).status).toBe(400);
  });

  test('one client cannot report installs without limit', async () => {
    const statuses: number[] = [];
    for (let i = 0; i < 41; i += 1) statuses.push((await post('/api/installs', beacon())).status);

    expect(statuses.filter(status => status === 202)).toHaveLength(40);
    expect(statuses.at(-1)).toBe(429);
  });

  test('the limit is per client, so one busy installer cannot silence everyone else', async () => {
    for (let i = 0; i < 40; i += 1) await post('/api/installs', beacon());

    expect((await post('/api/installs', beacon(), '198.51.100.4')).status).toBe(202);
  });

  test('reporting installs does not spend the budget for submitting sources', async () => {
    for (let i = 0; i < 40; i += 1) await post('/api/installs', beacon());

    const original = globalThis.fetch;
    globalThis.fetch = (() => Promise.reject(new TypeError('no network in tests'))) as unknown as typeof fetch;
    try {
      expect((await post('/api/sources', { url: 'https://listed.example/catalog.json' })).status).not.toBe(429);
    } finally {
      globalThis.fetch = original;
    }
  });

  test('the endpoint only takes posts', async () => {
    const request = new Request('https://bridgething.com/api/installs', { method: 'GET' });

    expect((await worker.fetch(request, env(), context())).status).toBe(404);
  });
});
