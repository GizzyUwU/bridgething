import type { MergedCatalog } from './directory-client';

export type StoreSource = { url: string; name: string; icon: string | null; official: boolean; attested: boolean };

export function vouchedFor(entry: MergedCatalog): boolean {
  return entry.official || entry.attested;
}

export function sourceOf(entry: MergedCatalog): StoreSource {
  return {
    url: entry.url,
    name: entry.catalog.repo.name,
    icon: entry.catalog.repo.icon,
    official: entry.official,
    attested: entry.attested,
  };
}

export function orderedByTrust(catalogs: MergedCatalog[]): MergedCatalog[] {
  return [...catalogs].sort((a, b) => Number(vouchedFor(b)) - Number(vouchedFor(a)));
}

export function sourceMap(catalogs: MergedCatalog[]): Map<string, StoreSource> {
  return new Map(catalogs.map(entry => [entry.url, sourceOf(entry)]));
}
