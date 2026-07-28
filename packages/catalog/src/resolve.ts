import type { AppEntry, AppVersion, Catalog, RecommendedSource, SourceCatalog } from './types.ts';
import { sortNewestFirst } from './versions.ts';

export type InstalledWebapp = {
  id: string;
  version: string;
  source: 'builtin' | 'installed';
  role: 'standard' | 'launcher';
  provenance?: string | null;
};

export type CatalogAppListing = {
  app: AppEntry;
  sourceUrl: string;
  newestCompatible: AppVersion | null;
  installedVersion: string | null;
  updateAvailable: boolean;
  alsoAvailableFrom: string[];
};

export type CatalogAppUpdate = {
  appId: string;
  name: string;
  installedVersion: string;
  target: AppVersion;
  sourceUrl: string;
};

export function satisfies(deviceVersion: string, minimum: string): boolean {
  return compareVersions(deviceVersion, minimum) >= 0;
}

export function isUpgrade(candidate: string, installed: string): boolean {
  return compareVersions(candidate, installed) > 0;
}

function compareVersions(a: string, b: string): number {
  const pa = versionComponents(a);
  const pb = versionComponents(b);
  for (let i = 0; i < Math.max(pa.length, pb.length); i += 1) {
    const x = pa[i] ?? 0;
    const y = pb[i] ?? 0;
    if (x !== y) return x < y ? -1 : 1;
  }
  return 0;
}

function versionComponents(raw: string): number[] {
  let v = raw.trim();
  if (v.startsWith('v') || v.startsWith('V')) v = v.slice(1);
  const cut = v.search(/[-+]/);
  if (cut !== -1) v = v.slice(0, cut);
  return v.split('.').map(part => {
    const n = Number.parseInt(part, 10);
    return Number.isNaN(n) ? 0 : n;
  });
}

export function pinsFrom(installed: InstalledWebapp[]): Map<string, string> {
  const out = new Map<string, string>();
  for (const info of installed) {
    if (info.provenance) out.set(info.id.toLowerCase(), info.provenance);
  }
  return out;
}

export function newestCompatible(app: AppEntry, deviceLibVersion: string | null): AppVersion | null {
  const ordered = sortNewestFirst(app.versions);
  if (deviceLibVersion === null) return ordered[0] ?? null;
  return ordered.find(v => satisfies(deviceLibVersion, v.min_libbridgething_version)) ?? null;
}

export function aggregate(args: {
  orderedCatalogs: SourceCatalog[];
  installed: InstalledWebapp[];
  deviceLibVersion: string | null;
}): CatalogAppListing[] {
  const { orderedCatalogs, installed, deviceLibVersion } = args;
  const installedById = new Map(installed.map(i => [i.id.toLowerCase(), i]));
  const pins = pinsFrom(installed);

  const offerings = new Map<string, { url: string; app: AppEntry }[]>();
  for (const { url, catalog } of orderedCatalogs) {
    for (const app of catalog.apps) {
      const list = offerings.get(app.id);
      if (list) list.push({ url, app });
      else offerings.set(app.id, [{ url, app }]);
    }
  }

  const listings: CatalogAppListing[] = [];
  for (const [id, offers] of offerings) {
    if (offers.length === 0) continue;
    const pinned = pins.get(id.toLowerCase());
    const primary = offers.find(o => o.url === pinned) ?? offers[0]!;
    const alsoAvailableFrom = offers.filter(o => o.url !== primary.url).map(o => o.url);

    const newest = newestCompatible(primary.app, deviceLibVersion);
    const installedVersion = installedById.get(id.toLowerCase())?.version ?? null;

    listings.push({
      app: primary.app,
      sourceUrl: primary.url,
      newestCompatible: newest,
      installedVersion,
      updateAvailable: installedVersion !== null && newest !== null && isUpgrade(newest.version, installedVersion),
      alsoAvailableFrom,
    });
  }

  return listings.sort((a, b) => a.app.name.localeCompare(b.app.name) || a.app.id.localeCompare(b.app.id));
}

export function updates(args: {
  catalogs: Map<string, Catalog>;
  installed: InstalledWebapp[];
  deviceLibVersion: string | null;
}): CatalogAppUpdate[] {
  const { catalogs, installed, deviceLibVersion } = args;
  const pins = pinsFrom(installed);
  const out: CatalogAppUpdate[] = [];

  for (const info of installed) {
    if (info.source !== 'installed' || info.role !== 'standard') continue;
    const id = info.id.toLowerCase();
    const sourceUrl = pins.get(id);
    if (!sourceUrl) continue;
    const app = catalogs.get(sourceUrl)?.apps.find(a => a.id.toLowerCase() === id);
    if (!app) continue;
    const newest = newestCompatible(app, deviceLibVersion);
    if (!newest || !isUpgrade(newest.version, info.version)) continue;
    out.push({
      appId: id,
      name: app.name,
      installedVersion: info.version,
      target: newest,
      sourceUrl,
    });
  }

  return out.sort((a, b) => a.name.localeCompare(b.name) || a.appId.localeCompare(b.appId));
}

export function recommendedSources(args: {
  orderedCatalogs: SourceCatalog[];
  subscribed: string[];
}): RecommendedSource[] {
  const subscribed = new Set(args.subscribed);
  const byUrl = new Map<string, RecommendedSource>();

  for (const { catalog } of args.orderedCatalogs) {
    for (const candidate of catalog.recommended_sources) {
      if (subscribed.has(candidate.url) || byUrl.has(candidate.url)) continue;
      byUrl.set(candidate.url, candidate);
    }
  }

  return [...byUrl.values()].sort((a, b) => Number(b.attested) - Number(a.attested) || a.name.localeCompare(b.name));
}
