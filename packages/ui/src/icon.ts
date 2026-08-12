export type ResolvedIcon = { kind: 'svg'; svg: string } | { kind: 'raster'; url: string } | { kind: 'failed' };

const MAX_BYTES = 64 * 1024;
const FETCH_TIMEOUT_MS = 10_000;

export const ICON_CACHE_LIMIT = 96;

const cache = new Map<string, ResolvedIcon>();

export function cachedIcon(key: string): ResolvedIcon | undefined {
  const hit = cache.get(key);
  if (!hit) return undefined;
  cache.delete(key);
  cache.set(key, hit);
  return hit;
}

export function cacheIcon(key: string, icon: ResolvedIcon): void {
  cache.delete(key);
  cache.set(key, icon);
  while (cache.size > ICON_CACHE_LIMIT) {
    const oldest = cache.keys().next();
    if (oldest.done) break;
    cache.delete(oldest.value);
  }
}

export function looksLikeSvg(contentType: string | null, body: string): boolean {
  if (contentType && /(^|[/+])svg/i.test(contentType)) return true;
  return /^\s*(<\?xml[^>]*\?>\s*)?(<!--.*?-->\s*)*<svg[\s/>]/is.test(body.slice(0, 512));
}

export function svgDataUrl(svg: string): string {
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}

export async function fetchIcon(url: string, signal?: AbortSignal): Promise<ResolvedIcon> {
  const abort = new AbortController();
  signal?.addEventListener('abort', () => abort.abort(), { once: true });
  const timer = setTimeout(() => abort.abort(), FETCH_TIMEOUT_MS);

  try {
    const response = await fetch(url, { signal: abort.signal });
    if (!response.ok) throw new Error(`icon fetch failed (${response.status})`);

    const declared = Number(response.headers.get('content-length'));
    if (Number.isFinite(declared) && declared > MAX_BYTES) throw new Error('icon larger than 64 KiB');

    const type = response.headers.get('content-type');
    const undecided = !type || /octet-stream|^text\/plain/i.test(type);
    if (!undecided && !/(^|[/+])svg/i.test(type)) {
      await response.body?.cancel();
      return { kind: 'raster', url };
    }

    const body = await response.text();
    if (body.length > MAX_BYTES) throw new Error('icon larger than 64 KiB');
    return looksLikeSvg(type, body) ? { kind: 'svg', svg: body } : { kind: 'raster', url };
  } catch {
    return { kind: 'failed' };
  } finally {
    clearTimeout(timer);
  }
}
