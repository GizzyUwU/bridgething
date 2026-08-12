import { OFFICIAL_CATALOG_URL, type MergedApps, type MergedCatalog } from '@bridgething/catalog';
import { byAttestedThenName, isPublished, type SourceRecord } from './directory.ts';
import { listInstalls, toInstallCounts } from './installs.ts';
import { fetchCatalogResponse, parseCatalogBody } from './probe.ts';
import { listSources, type KvLike } from './store.ts';

export { OFFICIAL_CATALOG_URL };
export type { MergedApps, MergedCatalog };

export const MAX_MERGED_SOURCES = 1024;

type Candidate = { url: string; official: boolean; attested: boolean };

function candidates(records: SourceRecord[]): Candidate[] {
  const published = records
    .filter(isPublished)
    .sort(byAttestedThenName)
    .map(record => ({ url: record.url, official: false, attested: record.status === 'attested' }));

  return [
    { url: OFFICIAL_CATALOG_URL, official: true, attested: true },
    ...published.filter(entry => entry.url !== OFFICIAL_CATALOG_URL),
  ];
}

async function fetchOne(
  candidate: Candidate,
  fetchImpl: typeof fetch,
): Promise<{ ok: true; merged: MergedCatalog } | { ok: false; url: string; reason: string }> {
  const fetched = await fetchCatalogResponse(candidate.url, fetchImpl);
  if (!fetched.ok) return { ok: false, url: candidate.url, reason: fetched.reason };

  const parsed = await parseCatalogBody(fetched.response, candidate.url);
  if (!parsed.ok) return { ok: false, url: candidate.url, reason: parsed.reason };

  return { ok: true, merged: { ...candidate, catalog: parsed.catalog } };
}

export async function mergedApps(args: { kv: KvLike; now: string; fetchImpl?: typeof fetch }): Promise<MergedApps> {
  const { kv, now, fetchImpl = fetch } = args;

  const all = candidates(await listSources(kv));
  const included = all.slice(0, MAX_MERGED_SOURCES);
  const skipped = all.slice(MAX_MERGED_SOURCES).map(entry => entry.url);

  const results = await Promise.all(included.map(candidate => fetchOne(candidate, fetchImpl)));

  const catalogs: MergedCatalog[] = [];
  const failures: { url: string; reason: string }[] = [];
  for (const result of results) {
    if (result.ok) catalogs.push(result.merged);
    else failures.push({ url: result.url, reason: result.reason });
  }

  return { updated_at: now, catalogs, failures, skipped, installs: toInstallCounts(await listInstalls(kv)) };
}
