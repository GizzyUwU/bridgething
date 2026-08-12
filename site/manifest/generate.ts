import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { parse as parseYaml } from 'yaml';
import { compose, composeVersion, type ComponentNotes } from './changelog.ts';
import { sortNewestFirst, type ComponentReleaseFile } from './sources.ts';
import type {
  Channel,
  ChannelSource,
  DiscoverManifest,
  ProjectSource,
  Release,
  ReleaseArtifacts,
  WakeWord,
} from './types.ts';
import { validate } from './validate.ts';

export type GenerateInput = {
  project: ProjectSource;
  channels: ChannelSource[];
  daemonReleases: ComponentReleaseFile[];
  imageReleases: ComponentReleaseFile[];
  bundles: BundleEntry[];
  updatedAt: string;
};

export type BundleEntry = {
  daemonVersion: string;
  imageVersion: string;
  channel: string;
  releasedAt: string;
  deprecated?: boolean;
  yanked?: string | null;
  daemonBumped: boolean;
  imageBumped: boolean;
  changelogUrl?: string | null;
  builtinWebapps?: Record<string, string>;
  wakeword?: WakeWord;
  artifacts?: ReleaseArtifacts;
  download: { url: string; size: number; sha256: string };
};

export function generate(input: GenerateInput): DiscoverManifest {
  const daemonByVersion = indexBy(input.daemonReleases, 'version');
  const imageByVersion = indexBy(input.imageReleases, 'version');
  const channelBySlug = indexBy(input.channels, 'slug');

  const releases: Record<string, Release> = {};
  const channelEntries: Record<string, BundleEntry[]> = {};

  for (const bundle of input.bundles) {
    const daemonNotes = lookupComponent(daemonByVersion, bundle.daemonVersion, 'daemon');
    const imageNotes = lookupComponent(imageByVersion, bundle.imageVersion, 'image');

    if (!channelBySlug[bundle.channel]) {
      throw new Error(`bundle references unknown channel "${bundle.channel}"`);
    }

    const composed = compose({
      daemon: toNotes(daemonNotes),
      image: toNotes(imageNotes),
      daemonBumped: bundle.daemonBumped,
      imageBumped: bundle.imageBumped,
    });

    const version = composeVersion(bundle.daemonVersion, bundle.imageVersion);
    if (releases[version]) {
      throw new Error(`duplicate bundle version "${version}"`);
    }

    releases[version] = {
      version,
      channel: bundle.channel,
      released_at: bundle.releasedAt,
      summary: composed.summary,
      changelog: composed.changelog,
      changelog_url: bundle.changelogUrl ?? null,
      yanked: bundle.yanked ?? null,
      deprecated: bundle.deprecated ?? false,
      ...(bundle.builtinWebapps && Object.keys(bundle.builtinWebapps).length > 0
        ? { builtin_webapps: bundle.builtinWebapps }
        : {}),
      ...(bundle.wakeword ? { wakeword: bundle.wakeword } : {}),
      ...(bundle.artifacts && Object.keys(bundle.artifacts).length > 0 ? { artifacts: bundle.artifacts } : {}),
      download: bundle.download,
    };

    (channelEntries[bundle.channel] ??= []).push(bundle);
  }

  const channels: Record<string, Channel> = {};
  for (const channel of input.channels) {
    const entries = sortNewestFirst(
      (channelEntries[channel.slug] ?? []).map(b => ({
        version: composeVersion(b.daemonVersion, b.imageVersion),
        released_at: b.releasedAt,
        installable: !b.yanked && !b.deprecated,
      })),
    );
    if (entries.length === 0) continue;
    channels[channel.slug] = {
      name: channel.name,
      description: channel.description,
      stability: channel.stability,
      default: channel.default,
      latest: (entries.find(e => e.installable) ?? entries[0]!).version,
      releases: entries.map(e => e.version),
    };
  }

  const manifest: DiscoverManifest = {
    $schema: 'https://terbium.app/schemas/manifest/v1.json',
    manifest_version: 1,
    updated_at: input.updatedAt,
    project: {
      ...input.project,
      screenshots: input.project.screenshots ?? [],
    },
    channels,
    releases,
  };

  return validate(manifest);
}

function indexBy<T, K extends keyof T>(items: T[], key: K): Record<string, T> {
  const out: Record<string, T> = {};
  for (const item of items) {
    const k = String(item[key]);
    if (out[k]) {
      throw new Error(`duplicate key "${k}" indexing by ${String(key)}`);
    }
    out[k] = item;
  }
  return out;
}

function lookupComponent(
  index: Record<string, ComponentReleaseFile>,
  version: string,
  kind: 'daemon' | 'image',
): ComponentReleaseFile {
  const entry = index[version];
  if (!entry) {
    throw new Error(`bundle references missing ${kind} release "${version}"`);
  }
  return entry;
}

function toNotes(file: ComponentReleaseFile): ComponentNotes {
  return { version: file.version, summary: file.summary, body: file.body };
}

export type ProjectAndChannelsConfig = {
  project: ProjectSource;
  channels: ChannelSource[];
};

export async function loadProjectAndChannels(configPath: string): Promise<ProjectAndChannelsConfig> {
  const raw = await readFile(configPath, 'utf-8');
  const parsed = parseYaml(raw) as ProjectAndChannelsConfig;
  if (!parsed.project) throw new Error(`${configPath}: missing "project" section`);
  if (!Array.isArray(parsed.channels) || parsed.channels.length === 0) {
    throw new Error(`${configPath}: "channels" must be a non-empty array`);
  }
  return parsed;
}

export async function loadBundles(bundlesPath: string): Promise<BundleEntry[]> {
  const raw = await readFile(bundlesPath, 'utf-8');
  const parsed = parseYaml(raw) as { bundles: BundleEntry[] };
  if (!Array.isArray(parsed?.bundles)) {
    throw new Error(`${bundlesPath}: expected "bundles" array`);
  }
  return parsed.bundles;
}

export function stringify(manifest: DiscoverManifest): string {
  return JSON.stringify(manifest, null, 2) + '\n';
}

export type { Channel, Release } from './types.ts';
export { join as joinPath };
