export type OtaManifestChannel = {
  name: string;
  stability: string;
  isDefault: boolean;
  latest: string;
  releases: string[];
};

export type ArtifactDigest = {
  size: number;
  sha256: string;
};

export type ReleaseArtifacts = {
  daemon?: ArtifactDigest;
  image_swu?: ArtifactDigest;
  image_zck?: ArtifactDigest;
  image_boot_zck?: ArtifactDigest;
  webapps?: Record<string, ArtifactDigest>;
  daemon_patches?: Record<string, ArtifactDigest>;
};

export type OtaManifestRelease = {
  version: string;
  channel: string;
  yanked: string | null;
  deprecated: boolean;
  builtinWebapps: Record<string, string>;
  artifacts?: ReleaseArtifacts;
};

export type OtaDiscoverManifest = {
  manifestVersion: number;
  updatedAt: string;
  channels: Record<string, OtaManifestChannel>;
  releases: Record<string, OtaManifestRelease>;
};

type RawChannel = {
  name: string;
  stability: string;
  default: boolean;
  latest: string;
  releases: string[];
};

type RawRelease = {
  version: string;
  channel: string;
  yanked: string | null;
  deprecated: boolean;
  builtin_webapps?: Record<string, string>;
  artifacts?: ReleaseArtifacts;
};

type RawManifest = {
  manifest_version: number;
  updated_at: string;
  channels: Record<string, RawChannel>;
  releases: Record<string, RawRelease>;
};

export class ManifestFetchError extends Error {
  constructor(
    message: string,
    public readonly status?: number,
  ) {
    super(message);
    this.name = 'ManifestFetchError';
  }
}

export async function fetchManifest(rootURL: string): Promise<OtaDiscoverManifest> {
  const response = await fetch(`${rootURL}/manifest.json`, { cache: 'no-store' });
  if (!response.ok) {
    throw new ManifestFetchError(`manifest fetch returned HTTP ${response.status}`, response.status);
  }
  const raw = (await response.json()) as RawManifest;
  return {
    manifestVersion: raw.manifest_version,
    updatedAt: raw.updated_at,
    channels: Object.fromEntries(
      Object.entries(raw.channels).map(([id, c]) => [
        id,
        { name: c.name, stability: c.stability, isDefault: c.default, latest: c.latest, releases: c.releases },
      ]),
    ),
    releases: Object.fromEntries(
      Object.entries(raw.releases).map(([version, r]) => [
        version,
        {
          version: r.version,
          channel: r.channel,
          yanked: r.yanked,
          deprecated: r.deprecated,
          builtinWebapps: r.builtin_webapps ?? {},
          artifacts: r.artifacts,
        },
      ]),
    ),
  };
}

export type OtaCompositeVersion = { daemon: string; image: string };

export function parseCompositeVersion(raw: string): OtaCompositeVersion | null {
  const plus = raw.indexOf('+');
  if (plus < 0) return null;
  const daemon = raw.slice(0, plus);
  const suffix = raw.slice(plus + 1);
  const prefix = 'image.';
  if (!suffix.startsWith(prefix)) return null;
  const image = suffix.slice(prefix.length);
  if (daemon.length === 0 || image.length === 0) return null;
  return { daemon, image };
}

export type OtaArtifactURLs = {
  daemonBinary: string;
  imageSwu: string;
  imageZck: string;
  imageBootZck: string;
};

export function imageVariantForChannel(channel: string): string {
  return channel === 'dev' ? 'dev' : 'prod';
}

export function otaArtifactUrls(opts: {
  rootURL: string;
  channel: string;
  daemonVersion: string;
  imageVersion: string;
  imageVariant: string;
}): OtaArtifactURLs {
  const imageName = `bridgething-${opts.imageVariant}-image`;
  const imagesDir = `${opts.rootURL}/images/${opts.channel}/${opts.imageVersion}`;
  return {
    daemonBinary: `${opts.rootURL}/daemon/${opts.channel}/${opts.daemonVersion}/bridgething`,
    imageSwu: `${imagesDir}/${imageName}.swu`,
    imageZck: `${imagesDir}/${imageName}.zck`,
    imageBootZck: `${imagesDir}/${imageName}-boot.zck`,
  };
}

export function daemonPatchUrl(opts: {
  rootURL: string;
  channel: string;
  toVersion: string;
  fromVersion: string;
}): string {
  return `${opts.rootURL}/daemon/${opts.channel}/${opts.toVersion}/patches/from-${opts.fromVersion}.zst`;
}

export function builtinWebappUrl(opts: { rootURL: string; channel: string; name: string; version: string }): string {
  return `${opts.rootURL}/webapps/${opts.channel}/${opts.name}/${opts.version}/${opts.name}.zip`;
}
