import { CatalogValidationError, validate, type Catalog } from '@bridgething/catalog';

export type SourceFetchResult =
  | { ok: true; catalog: Catalog }
  | { ok: false; kind: 'blocked'; message: string }
  | { ok: false; kind: 'http'; status: number; message: string }
  | { ok: false; kind: 'malformed'; message: string };

export function isCrossOrigin(url: string): boolean {
  if (typeof location === 'undefined') return true;
  try {
    return new URL(url, location.href).origin !== location.origin;
  } catch {
    return true;
  }
}

function blockedMessage(url: string): string {
  if (!isCrossOrigin(url)) return `could not reach ${url}. the host may be down.`;
  const origin = (() => {
    try {
      return new URL(url).origin;
    } catch {
      return url;
    }
  })();
  return (
    `could not read ${url} from the browser. the request was blocked before a response was readable, ` +
    `which almost always means ${origin} does not send the "Access-Control-Allow-Origin" header. ` +
    `a federated source must serve both its catalog and its download urls with "Access-Control-Allow-Origin: *". ` +
    `if the host is simply down, that looks identical from here.`
  );
}

export function relayUrl(sourceUrl: string): string {
  return `/api/catalog?url=${encodeURIComponent(sourceUrl)}`;
}

export async function fetchCatalog(
  url: string,
  init?: { signal?: AbortSignal; viaRelay?: boolean },
): Promise<SourceFetchResult> {
  if (init?.viaRelay) {
    const relayed = await fetchCatalogFrom(relayUrl(url), url, init.signal, { cache: 'default' });
    if (relayed.ok) return relayed;
  }
  return fetchCatalogFrom(url, url, init?.signal, { cache: 'no-store' });
}

async function fetchCatalogFrom(
  fetchUrl: string,
  url: string,
  signal: AbortSignal | undefined,
  options: { cache: RequestCache },
): Promise<SourceFetchResult> {
  let response: Response;
  try {
    response = await fetch(fetchUrl, { cache: options.cache, redirect: 'follow', signal });
  } catch (err) {
    if (err instanceof DOMException && err.name === 'AbortError') throw err;
    return { ok: false, kind: 'blocked', message: blockedMessage(url) };
  }

  if (!response.ok) {
    return {
      ok: false,
      kind: 'http',
      status: response.status,
      message: `${url} returned ${response.status} ${response.statusText}`.trimEnd(),
    };
  }

  let body: unknown;
  try {
    body = await response.json();
  } catch {
    return { ok: false, kind: 'malformed', message: `${url} did not return valid json` };
  }

  try {
    return { ok: true, catalog: validate(body) };
  } catch (err) {
    if (err instanceof CatalogValidationError) {
      return { ok: false, kind: 'malformed', message: `${url} is not a valid catalog.v1: ${err.message}` };
    }
    return {
      ok: false,
      kind: 'malformed',
      message: `${url} could not be validated: ${err instanceof Error ? err.message : String(err)}`,
    };
  }
}

export async function fetchAll(
  urls: string[],
  init?: { signal?: AbortSignal; relayable?: ReadonlySet<string> },
): Promise<{
  ordered: { url: string; catalog: Catalog }[];
  byUrl: Map<string, Catalog>;
  failures: { url: string; message: string }[];
}> {
  const results = await Promise.all(
    urls.map(url =>
      fetchCatalog(url, { signal: init?.signal, viaRelay: init?.relayable?.has(url) ?? false }).then(result => ({
        url,
        result,
      })),
    ),
  );

  const ordered: { url: string; catalog: Catalog }[] = [];
  const byUrl = new Map<string, Catalog>();
  const failures: { url: string; message: string }[] = [];

  for (const { url, result } of results) {
    if (result.ok) {
      ordered.push({ url, catalog: result.catalog });
      byUrl.set(url, result.catalog);
    } else {
      failures.push({ url, message: result.message });
    }
  }

  return { ordered, byUrl, failures };
}

export type BundleFetchResult =
  | { ok: true; blob: Blob }
  | { ok: false; kind: 'blocked'; message: string }
  | { ok: false; kind: 'http'; status: number; message: string }
  | { ok: false; kind: 'integrity'; message: string };

async function sha256Hex(bytes: ArrayBuffer): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  return Array.from(new Uint8Array(digest))
    .map(b => b.toString(16).padStart(2, '0'))
    .join('');
}

export async function fetchBundle(
  download: { url: string; size: number; sha256: string },
  init?: { signal?: AbortSignal },
): Promise<BundleFetchResult> {
  let response: Response;
  try {
    response = await fetch(download.url, { cache: 'no-store', redirect: 'follow', signal: init?.signal });
  } catch (err) {
    if (err instanceof DOMException && err.name === 'AbortError') throw err;
    return { ok: false, kind: 'blocked', message: blockedMessage(download.url) };
  }

  if (!response.ok) {
    return {
      ok: false,
      kind: 'http',
      status: response.status,
      message: `${download.url} returned ${response.status} ${response.statusText}`.trimEnd(),
    };
  }

  const bytes = await response.arrayBuffer();

  if (bytes.byteLength !== download.size) {
    return {
      ok: false,
      kind: 'integrity',
      message: `${download.url} is ${bytes.byteLength} bytes; the catalog says ${download.size}`,
    };
  }

  const actual = await sha256Hex(bytes);
  if (actual !== download.sha256.toLowerCase()) {
    return {
      ok: false,
      kind: 'integrity',
      message: `${download.url} sha256 is ${actual}; the catalog says ${download.sha256}. refusing to install it`,
    };
  }

  return { ok: true, blob: new Blob([bytes]) };
}
