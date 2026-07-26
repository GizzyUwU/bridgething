import {
  aggregate,
  recommendedSources as resolveRecommended,
  updates as resolveUpdates,
  validate,
  type Catalog,
  type CatalogAppListing,
  type CatalogAppUpdate,
  type InstalledWebapp,
  type RecommendedSource,
  type SourceCatalog,
} from '@bridgething/catalog';
import { create } from 'zustand';
import { useShallow } from 'zustand/react/shallow';

import { getSession } from './session';
import { useSessionStore } from './session';
import { storage } from './storage';
import { refreshWebapps, useWebappsStore } from './webapps';

export const OFFICIAL_CATALOG_URL = 'https://apps.bridgething.com/catalog.json';

export const SOURCE_DIRECTORY_URL = 'https://bridgething.com/api/sources.json';

const SOURCES_KEY = 'catalog.sources';

export type SourceFailure = { url: string; reason: string };

type CatalogState = {
  sources: string[];
  catalogs: SourceCatalog[];
  directory: Catalog | null;
  failures: SourceFailure[];
  refreshing: boolean;
};

type CatalogStore = CatalogState & {
  patch(next: Partial<CatalogState>): void;
};

const useCatalogStore = create<CatalogStore>(set => ({
  sources: loadSources(),
  catalogs: [],
  directory: null,
  failures: [],
  refreshing: false,
  patch: next => set(next),
}));

function loadSources(): string[] {
  const raw = storage.getString(SOURCES_KEY);
  if (!raw) return [OFFICIAL_CATALOG_URL];
  try {
    const parsed: unknown = JSON.parse(raw);
    const urls = Array.isArray(parsed)
      ? parsed.filter((u): u is string => typeof u === 'string')
      : [];
    return urls.length > 0 ? urls : [OFFICIAL_CATALOG_URL];
  } catch {
    return [OFFICIAL_CATALOG_URL];
  }
}

function saveSources(urls: string[]): void {
  storage.set(SOURCES_KEY, JSON.stringify(urls));
}

async function fetchCatalog(url: string): Promise<Catalog> {
  const response = await fetch(url, {
    headers: { accept: 'application/json' },
  });
  if (!response.ok) {
    throw new Error(`${url} returned ${response.status}`);
  }
  return validate(await response.json());
}

export async function refreshCatalog(): Promise<void> {
  const store = useCatalogStore.getState();
  store.patch({ refreshing: true });

  type Fetched = { url: string; catalog: Catalog } | SourceFailure;

  const results = await Promise.all<Fetched>(
    [...store.sources, SOURCE_DIRECTORY_URL].map(async url => {
      try {
        return { url, catalog: await fetchCatalog(url) };
      } catch (err) {
        return {
          url,
          reason: err instanceof Error ? err.message : String(err),
        };
      }
    }),
  );

  const catalogs: SourceCatalog[] = [];
  const failures: SourceFailure[] = [];
  let directory: Catalog | null = null;

  for (const result of results) {
    if ('reason' in result) {
      failures.push(result);
      continue;
    }
    if (result.url === SOURCE_DIRECTORY_URL) directory = result.catalog;
    else catalogs.push(result);
  }

  useCatalogStore.getState().patch({
    catalogs,
    directory,
    failures,
    refreshing: false,
  });
}

export async function addSource(url: string): Promise<void> {
  const trimmed = url.trim();
  const store = useCatalogStore.getState();
  if (!trimmed || store.sources.includes(trimmed)) return;
  const next = [...store.sources, trimmed];
  saveSources(next);
  store.patch({ sources: next });
  await refreshCatalog();
}

export async function removeSource(url: string): Promise<void> {
  const store = useCatalogStore.getState();
  if (!store.sources.includes(url)) return;
  const next = store.sources.filter(u => u !== url);
  saveSources(next);
  store.patch({
    sources: next,
    catalogs: store.catalogs.filter(entry => entry.url !== url),
  });
  await refreshCatalog();
}

function installedFor(deviceId: string | null): InstalledWebapp[] {
  if (!deviceId) return [];
  const entry = useWebappsStore.getState().byDevice[deviceId];
  return (entry?.list ?? []).map(info => ({
    id: info.id,
    version: info.version,
    source: info.source,
    role: info.role,
    provenance: info.provenance ?? null,
  }));
}

function deviceLibVersion(deviceId: string | null): string | null {
  if (!deviceId) return null;
  return (
    useSessionStore.getState().deviceMeta[deviceId]?.libbridgethingVersion ??
    null
  );
}

export function listingsFor(deviceId: string | null): CatalogAppListing[] {
  const { catalogs } = useCatalogStore.getState();
  return aggregate({
    orderedCatalogs: catalogs,
    installed: installedFor(deviceId),
    deviceLibVersion: deviceLibVersion(deviceId),
  });
}

export function updatesFor(deviceId: string): CatalogAppUpdate[] {
  const { catalogs } = useCatalogStore.getState();
  return resolveUpdates({
    catalogs: new Map(catalogs.map(entry => [entry.url, entry.catalog])),
    installed: installedFor(deviceId),
    deviceLibVersion: deviceLibVersion(deviceId),
  });
}

export function quickAddSources(): RecommendedSource[] {
  const { catalogs, directory, sources } = useCatalogStore.getState();
  const ordered: SourceCatalog[] = directory
    ? [{ url: SOURCE_DIRECTORY_URL, catalog: directory }, ...catalogs]
    : catalogs;
  return resolveRecommended({ orderedCatalogs: ordered, subscribed: sources });
}

export async function installApp(
  deviceId: string,
  listing: CatalogAppListing,
): Promise<void> {
  const version = listing.newestCompatible;
  if (!version) throw new Error('no compatible version to install');
  await getSession().installWebappFromUrl(
    deviceId,
    version.download.url,
    version.download.sha256,
    version.download.size,
    listing.sourceUrl,
  );
  await refreshWebapps(deviceId);
}

export function useCatalog<T>(selector: (state: CatalogState) => T): T {
  return useCatalogStore(useShallow(selector));
}
