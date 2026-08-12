import { CatalogValidationError, sortNewestFirst, validate, type Catalog } from '@bridgething/catalog';

export const SITE_ORIGIN = 'https://bridgething.com';
export const MAX_CATALOG_BYTES = 5 * 1024 * 1024;
export const PROBE_TIMEOUT_MS = 10_000;
const MAX_REPORTED_ERRORS = 5;

function summarize(err: unknown): string {
  if (!(err instanceof CatalogValidationError)) return String(err);
  const shown = err.errors.slice(0, MAX_REPORTED_ERRORS);
  const extra = err.errors.length - shown.length;
  return shown.join('; ') + (extra > 0 ? ` (and ${extra} more)` : '');
}

export type ProbeResult =
  | { ok: true; catalog: Catalog; downloadsCorsOk: boolean | null }
  | { ok: false; reason: string };

function corsPermits(headers: Headers): boolean {
  const allow = headers.get('access-control-allow-origin');
  if (allow === null) return false;
  const value = allow.trim();
  return value === '*' || value === SITE_ORIGIN;
}

async function readCapped(response: Response, maxBytes: number): Promise<string | null> {
  const declared = Number.parseInt(response.headers.get('content-length') ?? '', 10);
  if (!Number.isNaN(declared) && declared > maxBytes) return null;

  const body = response.body;
  if (!body) return null;

  const reader = body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    total += value.byteLength;
    if (total > maxBytes) {
      await reader.cancel();
      return null;
    }
    chunks.push(value);
  }

  const joined = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    joined.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return new TextDecoder().decode(joined);
}

async function probeDownloadCors(catalog: Catalog, fetchImpl: typeof fetch): Promise<boolean | null> {
  const first = catalog.apps[0];
  if (!first) return null;
  const newest = sortNewestFirst(first.versions)[0];
  if (!newest) return null;

  try {
    const response = await fetchImpl(newest.download.url, {
      method: 'HEAD',
      redirect: 'follow',
      headers: { origin: SITE_ORIGIN },
      signal: AbortSignal.timeout(PROBE_TIMEOUT_MS),
    });
    if (!response.ok) return false;
    return corsPermits(response.headers);
  } catch {
    return null;
  }
}

export type FetchedResponse = { ok: true; response: Response } | { ok: false; reason: string };

export async function fetchCatalogResponse(url: string, fetchImpl: typeof fetch = fetch): Promise<FetchedResponse> {
  let response: Response;
  try {
    response = await fetchImpl(url, {
      redirect: 'follow',
      headers: { accept: 'application/json', origin: SITE_ORIGIN },
      signal: AbortSignal.timeout(PROBE_TIMEOUT_MS),
    });
  } catch {
    return { ok: false, reason: `could not reach ${url}` };
  }

  if (!response.ok) {
    return { ok: false, reason: `${url} returned ${response.status} ${response.statusText}`.trimEnd() };
  }

  return { ok: true, response };
}

export type ParsedCatalog = { ok: true; catalog: Catalog } | { ok: false; reason: string };

export async function parseCatalogBody(response: Response, url: string): Promise<ParsedCatalog> {
  const body = await readCapped(response, MAX_CATALOG_BYTES);
  if (body === null) {
    return { ok: false, reason: `${url} is larger than ${MAX_CATALOG_BYTES} bytes` };
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(body);
  } catch {
    return { ok: false, reason: `${url} did not return valid json` };
  }

  try {
    return { ok: true, catalog: validate(parsed) };
  } catch (err) {
    return { ok: false, reason: `${url} is not a valid catalog.v1: ${summarize(err)}` };
  }
}

export function corsFailureReason(url: string): string {
  return (
    `${url} does not send "Access-Control-Allow-Origin". a source must serve its catalog and its ` +
    `download urls with "Access-Control-Allow-Origin: *" so a browser can read them.`
  );
}

export async function probeSource(url: string, fetchImpl: typeof fetch = fetch): Promise<ProbeResult> {
  const fetched = await fetchCatalogResponse(url, fetchImpl);
  if (!fetched.ok) return { ok: false, reason: fetched.reason };

  if (!corsPermits(fetched.response.headers)) {
    return { ok: false, reason: corsFailureReason(url) };
  }

  const parsed = await parseCatalogBody(fetched.response, url);
  if (!parsed.ok) return { ok: false, reason: parsed.reason };

  return { ok: true, catalog: parsed.catalog, downloadsCorsOk: await probeDownloadCors(parsed.catalog, fetchImpl) };
}
