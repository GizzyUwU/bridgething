import { isPublished, normalizeSourceUrl, SourceUrlError } from './directory.ts';
import { fetchCatalogResponse, parseCatalogBody } from './probe.ts';
import { listSources, type KvLike } from './store.ts';

export type RelayOutcome = { ok: true; catalog: unknown; url: string } | { ok: false; status: number; reason: string };

export async function relayCatalog(args: {
  kv: KvLike;
  rawUrl: string | null;
  fetchImpl?: typeof fetch;
}): Promise<RelayOutcome> {
  const { kv, rawUrl, fetchImpl } = args;

  if (!rawUrl) return { ok: false, status: 400, reason: 'pass the source as ?url=' };

  let url: string;
  try {
    url = normalizeSourceUrl(rawUrl);
  } catch (err) {
    if (err instanceof SourceUrlError) return { ok: false, status: 400, reason: err.message };
    throw err;
  }

  const record = (await listSources(kv)).find(entry => entry.url === url);
  if (!record || !isPublished(record)) {
    return { ok: false, status: 404, reason: `${url} is not a listed source; fetch it directly` };
  }

  const fetched = await fetchCatalogResponse(url, fetchImpl);
  if (!fetched.ok) return { ok: false, status: 502, reason: fetched.reason };

  const parsed = await parseCatalogBody(fetched.response, url);
  if (!parsed.ok) return { ok: false, status: 502, reason: parsed.reason };

  return { ok: true, catalog: parsed.catalog, url };
}
