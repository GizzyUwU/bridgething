import { APP_DETAIL_SHELL, appIdFromPath } from '../src/lib/app-routes.ts';
import { mergedApps } from './apps.ts';
import { isVisible, SOURCE_STATUSES, toCatalogDocument, toDirectoryView, type SourceStatus } from './directory.ts';
import { rebuildInstalls, recordInstall } from './installs.ts';
import { relayCatalog } from './relay.ts';
import { recheckSource, setSourceStatus, submitSource } from './sources.ts';
import { listSources, rebuildSources, takeRateLimitToken, type KvLike } from './store.ts';

export type Env = {
  SOURCES: KVNamespace;
  ASSETS: Fetcher;
  ADMIN_TOKEN: string;
};

const SUBMIT_LIMIT = 5;
const SUBMIT_WINDOW_SECONDS = 3600;
const INSTALL_LIMIT = 40;
const INSTALL_WINDOW_SECONDS = 3600;
const RECHECK_BATCH = 20;

const CORS_HEADERS: Record<string, string> = {
  'access-control-allow-origin': '*',
  'access-control-allow-methods': 'GET, POST, PATCH, OPTIONS',
  'access-control-allow-headers': 'content-type, authorization',
  'access-control-max-age': '86400',
};

function json(body: unknown, init: { status?: number; cache?: string } = {}): Response {
  const headers: Record<string, string> = {
    'content-type': 'application/json; charset=utf-8',
    ...CORS_HEADERS,
  };
  if (init.cache) headers['cache-control'] = init.cache;
  return new Response(JSON.stringify(body, null, 2), { status: init.status ?? 200, headers });
}

function fail(status: number, reason: string): Response {
  return json({ error: reason }, { status });
}

const CACHED_ROUTES = ['/api/sources.json', '/api/directory.json', '/api/apps.json', '/api/catalog'];
const DIRECTORY_ROUTES = ['/api/sources.json', '/api/directory.json', '/api/apps.json'];

export function relayPath(sourceUrl: string): string {
  return `/api/catalog?url=${encodeURIComponent(sourceUrl)}`;
}

type EdgeCache = {
  match(request: Request): Promise<Response | undefined>;
  put(request: Request, response: Response): Promise<void>;
  delete(request: Request): Promise<boolean>;
};

const edgeCache = (caches as unknown as { default: EdgeCache }).default;

async function dropCached(origin: string, sourceUrl?: string): Promise<void> {
  const targets = DIRECTORY_ROUTES.map(route => origin + route);
  if (sourceUrl) targets.push(origin + relayPath(sourceUrl));
  await Promise.all(targets.map(target => edgeCache.delete(new Request(target))));
}

function tokenMatches(provided: string, expected: string): boolean {
  if (!expected) return false;
  const a = new TextEncoder().encode(provided);
  const b = new TextEncoder().encode(expected);
  let diff = a.byteLength ^ b.byteLength;
  for (let i = 0; i < Math.max(a.byteLength, b.byteLength); i += 1) {
    diff |= (a[i] ?? 0) ^ (b[i] ?? 0);
  }
  return diff === 0;
}

function authorized(request: Request, env: Env): boolean {
  const header = request.headers.get('authorization') ?? '';
  const prefix = 'Bearer ';
  if (!header.startsWith(prefix)) return false;
  return tokenMatches(header.slice(prefix.length).trim(), env.ADMIN_TOKEN ?? '');
}

async function readJsonBody(request: Request): Promise<Record<string, unknown> | null> {
  try {
    const body = await request.json();
    return body !== null && typeof body === 'object' ? (body as Record<string, unknown>) : null;
  } catch {
    return null;
  }
}

async function handleApi(request: Request, env: Env, url: URL): Promise<Response> {
  const kv = env.SOURCES as unknown as KvLike;
  const now = new Date().toISOString();

  if (request.method === 'OPTIONS') return new Response(null, { status: 204, headers: CORS_HEADERS });

  if (url.pathname === '/api/sources.json' && request.method === 'GET') {
    const records = await listSources(kv);
    return json(toCatalogDocument(records, now), { cache: 'public, max-age=300' });
  }

  if (url.pathname === '/api/directory.json' && request.method === 'GET') {
    const records = await listSources(kv);
    return json({ updated_at: now, sources: toDirectoryView(records) }, { cache: 'public, max-age=60' });
  }

  if (url.pathname === '/api/apps.json' && request.method === 'GET') {
    return json(await mergedApps({ kv, now }), { cache: 'public, max-age=300' });
  }

  if (url.pathname === '/api/catalog' && request.method === 'GET') {
    const outcome = await relayCatalog({ kv, rawUrl: url.searchParams.get('url') });
    if (!outcome.ok) return fail(outcome.status, outcome.reason);
    return json(outcome.catalog, { cache: 'public, max-age=300' });
  }

  if (url.pathname === '/api/sources' && request.method === 'POST') {
    const body = await readJsonBody(request);
    const raw = body?.['url'];
    if (typeof raw !== 'string' || !raw.trim()) return fail(400, 'send a json body with a "url" string');

    const client = request.headers.get('cf-connecting-ip') ?? 'unknown';
    if (!(await takeRateLimitToken(kv, `submit:${client}`, SUBMIT_LIMIT, SUBMIT_WINDOW_SECONDS))) {
      return fail(429, `at most ${SUBMIT_LIMIT} submissions per hour. try again later.`);
    }

    const outcome = await submitSource({ kv, rawUrl: raw, now });
    if (!outcome.ok) return fail(outcome.status, outcome.reason);
    await dropCached(url.origin, outcome.record.url);
    return json({ source: outcome.record }, { status: outcome.created ? 201 : 200 });
  }

  if (url.pathname === '/api/installs' && request.method === 'POST') {
    const client = request.headers.get('cf-connecting-ip') ?? 'unknown';
    if (!(await takeRateLimitToken(kv, `install:${client}`, INSTALL_LIMIT, INSTALL_WINDOW_SECONDS))) {
      return fail(429, `at most ${INSTALL_LIMIT} install reports per hour. try again later.`);
    }

    const outcome = await recordInstall({ kv, body: await readJsonBody(request), now });
    if (!outcome.ok) return fail(outcome.status, outcome.reason);
    return json({ installs: outcome.record.count }, { status: 202 });
  }

  if (url.pathname === '/api/admin/sources') {
    if (!authorized(request, env)) return fail(401, 'admin token required');

    if (request.method === 'GET') {
      return json({ sources: await listSources(kv) }, { cache: 'no-store' });
    }

    if (request.method === 'PATCH') {
      const body = await readJsonBody(request);
      const raw = body?.['url'];
      const status = body?.['status'];
      const note = body?.['note'];

      if (typeof raw !== 'string' || !raw.trim()) return fail(400, 'send a json body with a "url" string');
      if (typeof status !== 'string' || !SOURCE_STATUSES.includes(status as SourceStatus)) {
        return fail(400, `"status" must be one of ${SOURCE_STATUSES.join(', ')}`);
      }
      if (note !== undefined && note !== null && typeof note !== 'string') {
        return fail(400, '"note" must be a string or null');
      }

      const outcome = await setSourceStatus({
        kv,
        rawUrl: raw,
        status: status as SourceStatus,
        note: note as string | null | undefined,
        now,
      });
      if (!outcome.ok) return fail(outcome.status, outcome.reason);
      await dropCached(url.origin, outcome.record.url);
      return json({ source: outcome.record });
    }
  }

  return fail(404, `no api route for ${request.method} ${url.pathname}`);
}

export default {
  async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname !== '/api' && !url.pathname.startsWith('/api/')) {
      const asset = await env.ASSETS.fetch(request);
      if (asset.status !== 404 || appIdFromPath(url.pathname) === null) return asset;
      return env.ASSETS.fetch(new Request(new URL(APP_DETAIL_SHELL, url.origin), request));
    }

    const head = request.method === 'HEAD';
    const effective = head ? new Request(request.url, { method: 'GET', headers: request.headers }) : request;

    const cacheable = effective.method === 'GET' && CACHED_ROUTES.includes(url.pathname);
    if (cacheable) {
      const hit = await edgeCache.match(effective);
      if (hit) return head ? new Response(null, hit) : hit;
    }

    const response = await handleApi(effective, env, url);
    if (cacheable && response.ok) {
      ctx.waitUntil(edgeCache.put(effective, response.clone()));
    }
    return head ? new Response(null, response) : response;
  },

  async scheduled(_event: ScheduledController, env: Env): Promise<void> {
    const kv = env.SOURCES as unknown as KvLike;
    const now = new Date().toISOString();

    await rebuildInstalls(kv);

    const stalest = (await rebuildSources(kv))
      .filter(isVisible)
      .sort((a, b) => a.last_checked_at.localeCompare(b.last_checked_at))
      .slice(0, RECHECK_BATCH);

    for (const record of stalest) {
      await recheckSource({ kv, record, now });
    }
  },
};
