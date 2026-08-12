import type { Download, RecommendedSource, Repo } from '@bridgething/catalog';

export type CatalogCuration = {
  repo: Repo;
  apps?: AppCurationEntry[];
};

export type AppCurationEntry = {
  slug: string;
  author?: string;
  name?: string;
  description?: string;
  icon?: string | null;
  homepage?: string | null;
  source?: string | null;
};

export type PublishedState = {
  recommended_sources?: RecommendedSource[];
  apps?: PublishedAppEntry[];
};

export type PublishedAppEntry = {
  slug: string;
  id: string;
  name: string;
  description: string;
  icon: string | null;
  versions: AppVersionConfig[];
};

export type AppConfigEntry = {
  slug: string;
  id: string;
  name: string;
  description: string;
  author: string;
  icon: string | null;
  homepage?: string | null;
  source?: string | null;
  versions: AppVersionConfig[];
};

export type AppVersionConfig = {
  version: string;
  released_at: string;
  download: Download;
  permissions: string[];
  min_libbridgething_version: string;
  changelog?: string | null;
};
