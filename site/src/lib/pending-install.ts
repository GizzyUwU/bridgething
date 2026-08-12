import type { AppEntry, AppVersion, CatalogAppListing } from '@bridgething/catalog';

export type PendingInstall = {
  appId: string;
  name: string;
  version: string;
  download: { url: string; size: number; sha256: string };
  minLibbridgethingVersion: string;
  provenance: string;
};

const KEY = 'bridgething:pending-install';
const PLACEHOLDER_SHA256 = '0'.repeat(64);

export function isPlaceholderDownload(download: { size: number; sha256: string }): boolean {
  return download.size <= 0 || download.sha256 === PLACEHOLDER_SHA256;
}

export function toPendingInstall(app: AppEntry, version: AppVersion, sourceUrl: string): PendingInstall {
  return {
    appId: app.id,
    name: app.name,
    version: version.version,
    download: {
      url: version.download.url,
      size: version.download.size,
      sha256: version.download.sha256,
    },
    minLibbridgethingVersion: version.min_libbridgething_version,
    provenance: sourceUrl,
  };
}

function isPendingInstall(value: unknown): value is PendingInstall {
  if (typeof value !== 'object' || value === null) return false;
  const v = value as Record<string, unknown>;
  const download = v.download as Record<string, unknown> | undefined;
  return (
    typeof v.appId === 'string' &&
    typeof v.name === 'string' &&
    typeof v.version === 'string' &&
    typeof v.minLibbridgethingVersion === 'string' &&
    typeof v.provenance === 'string' &&
    typeof download === 'object' &&
    download !== null &&
    typeof download.url === 'string' &&
    typeof download.size === 'number' &&
    typeof download.sha256 === 'string'
  );
}

export function parsePendingInstall(raw: string): PendingInstall | null {
  try {
    const parsed: unknown = JSON.parse(raw);
    return isPendingInstall(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

export function stagePendingInstall(pending: PendingInstall): void {
  if (typeof sessionStorage === 'undefined') return;
  sessionStorage.setItem(KEY, JSON.stringify(pending));
}

export function installListing(listing: CatalogAppListing, destination = '/device'): void {
  if (!listing.newestCompatible) return;
  stagePendingInstall(toPendingInstall(listing.app, listing.newestCompatible, listing.sourceUrl));
  window.location.href = destination;
}

export function takePendingInstall(): PendingInstall | null {
  if (typeof sessionStorage === 'undefined') return null;
  const raw = sessionStorage.getItem(KEY);
  if (raw === null) return null;
  sessionStorage.removeItem(KEY);
  return parsePendingInstall(raw);
}
