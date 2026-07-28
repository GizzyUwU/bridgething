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
import type { BridgethingWebappInfo } from '@bridgething/session-react-native';
import { useMemo } from 'react';
import { create } from 'zustand';
import { useShallow } from 'zustand/react/shallow';

import { getSession } from './bridge';
import { useSessionStore, type SessionState } from './session';
import { storage } from './storage';
import { useWebappsStore } from './webapps';

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
  previews: Record<string, { catalog: Catalog; fetchedAt: number }>;
};

const PREVIEW_TTL_MS = 5 * 60 * 1000;

export const useCatalogStore = create<CatalogState>(() => ({
  sources: loadSources(),
  catalogs: [],
  directory: null,
  failures: [],
  refreshing: false,
  previews: {},
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

export async function fetchCatalog(url: string): Promise<Catalog> {
  const response = await fetch(url, {
    headers: { accept: 'application/json' },
  });
  if (!response.ok) throw new Error(`${url} returned ${response.status}`);
  return validate(await response.json());
}

let refreshGeneration = 0;

export async function refreshCatalog(): Promise<void> {
  const generation = ++refreshGeneration;
  const { sources } = useCatalogStore.getState();
  useCatalogStore.setState({ refreshing: true });

  type Fetched = { url: string; catalog: Catalog } | SourceFailure;
  const results = await Promise.all<Fetched>(
    [...sources, SOURCE_DIRECTORY_URL].map(async url => {
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
    if ('reason' in result) failures.push(result);
    else if (result.url === SOURCE_DIRECTORY_URL) directory = result.catalog;
    else catalogs.push(result);
  }

  if (generation !== refreshGeneration) return;
  useCatalogStore.setState({
    catalogs,
    directory,
    failures,
    refreshing: false,
  });
}

export async function previewSource(url: string): Promise<Catalog> {
  const cached = useCatalogStore.getState().previews[url];
  if (cached && Date.now() - cached.fetchedAt < PREVIEW_TTL_MS) {
    return cached.catalog;
  }
  const catalog = await fetchCatalog(url);
  useCatalogStore.setState(s => ({
    previews: { ...s.previews, [url]: { catalog, fetchedAt: Date.now() } },
  }));
  return catalog;
}

export function usePreview(url: string | null): Catalog | null {
  return useCatalogStore(s =>
    url ? (s.previews[url]?.catalog ?? null) : null,
  );
}

export function useIsSubscribed(url: string): boolean {
  return useCatalogStore(s => s.sources.includes(url));
}

export async function addSource(url: string): Promise<void> {
  const trimmed = url.trim();
  const { sources } = useCatalogStore.getState();
  if (!trimmed || sources.includes(trimmed)) return;
  const next = [...sources, trimmed];
  saveSources(next);
  useCatalogStore.setState({ sources: next });
  await refreshCatalog();
}

export async function removeSource(url: string): Promise<void> {
  const { sources, catalogs } = useCatalogStore.getState();
  if (!sources.includes(url)) return;
  const next = sources.filter(u => u !== url);
  saveSources(next);
  useCatalogStore.setState({
    sources: next,
    catalogs: catalogs.filter(e => e.url !== url),
  });
  await refreshCatalog();
}

function toInstalled(list: BridgethingWebappInfo[]): InstalledWebapp[] {
  return list.map(info => ({
    id: info.id,
    version: info.version,
    source: info.source,
    role: info.role,
    provenance: info.provenance ?? null,
  }));
}

export function deviceLibVersion(
  state: SessionState,
  deviceId: string | null,
): string | null {
  if (!deviceId) return null;
  return state.ledger[deviceId]?.libVersion ?? null;
}

function useDerivedInputs(deviceId: string | null) {
  const catalogs = useCatalogStore(s => s.catalogs);
  const installed = useWebappsStore(
    useShallow(s => (deviceId ? (s.byDevice[deviceId]?.list ?? []) : [])),
  );
  const libVersion = useSessionStore(s => deviceLibVersion(s, deviceId));
  return { catalogs, installed, deviceLibVersion: libVersion };
}

export function useListings(deviceId: string | null): CatalogAppListing[] {
  const { catalogs, installed, deviceLibVersion } = useDerivedInputs(deviceId);
  return useMemo(
    () =>
      aggregate({
        orderedCatalogs: catalogs,
        installed: toInstalled(installed),
        deviceLibVersion,
      }),
    [catalogs, installed, deviceLibVersion],
  );
}

export function useSourceListings(
  url: string | null,
  deviceId: string | null,
): CatalogAppListing[] {
  const preview = usePreview(url);
  const { catalogs, installed, deviceLibVersion } = useDerivedInputs(deviceId);
  return useMemo(() => {
    if (!url) return [];
    const catalog = preview ?? catalogs.find(c => c.url === url)?.catalog;
    if (!catalog) return [];
    return aggregate({
      orderedCatalogs: [{ url, catalog }],
      installed: toInstalled(installed),
      deviceLibVersion,
    });
  }, [url, preview, catalogs, installed, deviceLibVersion]);
}

export function useUpdates(deviceId: string | null): CatalogAppUpdate[] {
  const { catalogs, installed, deviceLibVersion } = useDerivedInputs(deviceId);
  return useMemo(
    () =>
      resolveUpdates({
        catalogs: new Map(catalogs.map(e => [e.url, e.catalog])),
        installed: toInstalled(installed),
        deviceLibVersion,
      }),
    [catalogs, installed, deviceLibVersion],
  );
}

export function useQuickAddSources(): RecommendedSource[] {
  const catalogs = useCatalogStore(s => s.catalogs);
  const directory = useCatalogStore(s => s.directory);
  const sources = useCatalogStore(s => s.sources);

  return useMemo(() => {
    const ordered: SourceCatalog[] = directory
      ? [{ url: SOURCE_DIRECTORY_URL, catalog: directory }, ...catalogs]
      : catalogs;
    return resolveRecommended({
      orderedCatalogs: ordered,
      subscribed: sources,
    });
  }, [catalogs, directory, sources]);
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
    listing.app.id,
    listing.app.name,
  );
}

export function useCatalog<T>(selector: (state: CatalogState) => T): T {
  return useCatalogStore(useShallow(selector));
}
