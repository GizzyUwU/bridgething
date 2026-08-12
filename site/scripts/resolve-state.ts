#!/usr/bin/env bun
import { compareVersions, satisfies } from '@bridgething/catalog';
import { appendFile, readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { parse } from 'yaml';
import { parseVersion } from '../manifest/changelog.ts';
import { readComponentReleases, type ComponentReleaseFile } from '../manifest/sources.ts';
import type { DiscoverManifest } from '../manifest/types.ts';

interface Args {
  current: string;
  channel: string;
  kind: 'daemon' | 'image';
  daemonVersion?: string;
  imageVersion?: string;
  daemonNotes: string;
  imageNotes: string;
  force: boolean;
}

function parseArgs(argv: string[]): Args {
  const out: Partial<Args> = {};
  for (let i = 0; i < argv.length; i++) {
    const flag = argv[i];
    const next = argv[i + 1];
    if (!next) continue;
    switch (flag) {
      case '--current':
        out.current = next;
        break;
      case '--channel':
        out.channel = next;
        break;
      case '--kind':
        if (next !== 'daemon' && next !== 'image') {
          throw new Error(`--kind must be daemon|image, got "${next}"`);
        }
        out.kind = next;
        break;
      case '--daemon-version':
        out.daemonVersion = next || undefined;
        break;
      case '--image-version':
        out.imageVersion = next || undefined;
        break;
      case '--daemon-notes':
        out.daemonNotes = next;
        break;
      case '--image-notes':
        out.imageNotes = next;
        break;
    }
  }
  out.force = argv.includes('--force');
  for (const k of ['current', 'channel', 'kind', 'daemonNotes', 'imageNotes'] as const) {
    if (!out[k]) throw new Error(`missing --${k.replace(/[A-Z]/g, c => `-${c.toLowerCase()}`)}`);
  }
  return out as Args;
}

function currentChannelLatest(manifest: DiscoverManifest, channel: string): { daemon: string; image: string } | null {
  const ch = manifest.channels?.[channel];
  if (!ch) return null;
  return parseVersion(ch.latest);
}

function meetsFloor(floor: string | null | undefined, have: string): boolean {
  return !floor || satisfies(have, floor);
}

function noteFor(notes: ComponentReleaseFile[], version: string): ComponentReleaseFile | undefined {
  return notes.find(n => n.version === version);
}

function newestEligibleDaemon(
  daemonNotes: ComponentReleaseFile[],
  imageVersion: string,
  imageFloor: string | null | undefined,
  previousDaemon: string | undefined,
): string | null {
  const eligible = daemonNotes.filter(
    n => meetsFloor(n.min_image_version, imageVersion) && meetsFloor(imageFloor, n.version),
  );
  const candidates = previousDaemon ? eligible.filter(n => compareVersions(n.version, previousDaemon) >= 0) : eligible;
  if (candidates.length === 0) return null;
  return candidates.reduce((best, n) => (compareVersions(n.version, best.version) > 0 ? n : best)).version;
}

async function existingBundleRow(
  channel: string,
  daemonVersion: string,
  imageVersion: string,
): Promise<{ daemonBumped?: boolean; imageBumped?: boolean } | null> {
  const path = resolve(import.meta.dirname, '..', 'manifest', 'bundles.yaml');
  const raw = await readFile(path, 'utf-8').catch(() => null);
  if (raw === null) return null;
  const doc = parse(raw) as { bundles?: Record<string, unknown>[] };
  const row = (doc.bundles ?? []).find(
    b => b['channel'] === channel && b['daemonVersion'] === daemonVersion && b['imageVersion'] === imageVersion,
  );
  if (!row) return null;
  return { daemonBumped: row['daemonBumped'] as boolean, imageBumped: row['imageBumped'] as boolean };
}

async function emit(lines: string[]): Promise<void> {
  const out = process.env['GITHUB_OUTPUT'];
  if (out) await appendFile(out, lines.join('\n') + '\n');
  console.log(lines.join('\n'));
}

async function hold(reason: string): Promise<never> {
  console.log(`holding: ${reason}`);
  await emit(['released=false']);
  process.exit(0);
}

const args = parseArgs(process.argv.slice(2));
const raw = await readFile(args.current, 'utf-8').catch(() => '{}');
const current = JSON.parse(raw) as DiscoverManifest;
const previous = currentChannelLatest(current, args.channel);
const [daemonNotes, imageNotes] = await Promise.all([
  readComponentReleases(args.daemonNotes),
  readComponentReleases(args.imageNotes),
]);

let daemonVersion: string;
let imageVersion: string;

if (args.kind === 'daemon') {
  if (!args.daemonVersion) throw new Error('--daemon-version required for kind=daemon');
  daemonVersion = args.daemonVersion;
  imageVersion = args.imageVersion ?? previous?.image ?? '';
  if (!imageVersion) {
    throw new Error(
      `daemon trigger on channel "${args.channel}" but no current image release; cut an image release first`,
    );
  }
} else {
  if (!args.imageVersion) throw new Error('--image-version required for kind=image');
  imageVersion = args.imageVersion;
  const imageFloor = noteFor(imageNotes, imageVersion)?.min_daemon_version;
  daemonVersion =
    args.daemonVersion ?? newestEligibleDaemon(daemonNotes, imageVersion, imageFloor, previous?.daemon) ?? '';
  if (!daemonVersion) {
    throw new Error(
      `image ${imageVersion} on channel "${args.channel}" needs daemon >= ${imageFloor ?? '(any)'}, ` +
        `but no released daemon satisfies it (channel currently ships ${previous?.daemon ?? 'nothing'})`,
    );
  }
}

const daemonFloor = noteFor(daemonNotes, daemonVersion)?.min_image_version;
if (!meetsFloor(daemonFloor, imageVersion)) {
  await hold(
    `daemon ${daemonVersion} needs image >= ${daemonFloor}, channel "${args.channel}" is on ${imageVersion}. ` +
      `Its artifacts stay on R2 unreferenced; cutting that image publishes the pair.`,
  );
}
const imageFloor = noteFor(imageNotes, imageVersion)?.min_daemon_version;
if (!meetsFloor(imageFloor, daemonVersion)) {
  throw new Error(
    `image ${imageVersion} needs daemon >= ${imageFloor} but resolved ${daemonVersion}; release that daemon first`,
  );
}

let daemonBumped = previous?.daemon !== daemonVersion;
let imageBumped = previous?.image !== imageVersion;

if (!daemonBumped && !imageBumped) {
  if (!args.force) {
    await hold(`daemon=${daemonVersion}, image=${imageVersion} already is the "${args.channel}" latest`);
  }
  const row = await existingBundleRow(args.channel, daemonVersion, imageVersion);
  daemonBumped = row?.daemonBumped ?? true;
  imageBumped = row?.imageBumped ?? true;
  console.log(`forcing a recompose of ${daemonVersion}+image.${imageVersion} on "${args.channel}"`);
}

await emit([
  `daemon_version=${daemonVersion}`,
  `image_version=${imageVersion}`,
  `daemon_bumped=${daemonBumped ? 'true' : 'false'}`,
  `image_bumped=${imageBumped ? 'true' : 'false'}`,
  `released=true`,
]);
