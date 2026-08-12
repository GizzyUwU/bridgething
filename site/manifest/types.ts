export type DiscoverManifest = {
  $schema?: string;
  manifest_version: 1;
  updated_at: string;
  project: Project;
  channels: Record<string, Channel>;
  releases: Record<string, Release>;
};

export type Project = {
  id: string;
  name: string;
  description: string;
  publisher: string;
  publisher_url: string | null;
  license: string | null;
  website: string | null;
  source_url: string | null;
  issue_url: string | null;
  support_url: string | null;
  icon_url: string | null;
  banner_url: string | null;
  screenshots: Screenshot[];
};

export type Screenshot = {
  url: string;
  caption: string | null;
  alt: string | null;
};

export type Channel = {
  name: string;
  description: string;
  stability: 'stable' | 'beta' | 'experimental';
  default: boolean;
  latest: string;
  releases: string[];
};

export type Release = {
  version: string;
  channel: string;
  released_at: string;
  summary: string;
  changelog: string;
  changelog_url: string | null;
  yanked: string | null;
  deprecated: boolean;
  builtin_webapps?: Record<string, string>;
  wakeword?: WakeWord;
  artifacts?: ReleaseArtifacts;
  download: Download;
};

export type WakeWord = {
  runtime: string;
  model: string;
  model_trained_against?: Record<string, string>;
};

export type Download = {
  url: string;
  size: number;
  sha256: string;
};

export type ArtifactDigest = {
  size: number;
  sha256: string;
};

export type PatchDigest = ArtifactDigest & {
  source_sha256?: string;
};

export type ReleaseArtifacts = {
  daemon?: ArtifactDigest;
  daemon_zst?: ArtifactDigest;
  image_swu?: ArtifactDigest;
  image_zck?: ArtifactDigest;
  image_boot_zck?: ArtifactDigest;
  webapps?: Record<string, ArtifactDigest>;
  wakeword?: WakeWordArtifacts;
  daemon_patches?: Record<string, PatchDigest>;
};

export type WakeWordArtifacts = {
  runtime?: ArtifactDigest;
  model?: ArtifactDigest;
};

export type ReleaseSource = {
  version: string;
  channel: string;
  released_at: string;
  daemon_version: string;
  image_version: string;
  daemon_bumped: boolean;
  image_bumped: boolean;
  summary: string;
  changelog: string;
  changelog_url: string | null;
  yanked: string | null;
  deprecated: boolean;
  builtin_webapps?: Record<string, string>;
  wakeword?: WakeWord;
  artifacts?: ReleaseArtifacts;
  download: Download;
};

export type ChannelSource = {
  slug: string;
  name: string;
  description: string;
  stability: 'stable' | 'beta' | 'experimental';
  default: boolean;
};

export type ProjectSource = Omit<Project, 'screenshots'> & {
  screenshots?: Screenshot[];
};
