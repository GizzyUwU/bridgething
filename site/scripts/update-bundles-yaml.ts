#!/usr/bin/env bun
import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { parse, stringify } from 'yaml';
import { composeVersion } from '../manifest/changelog.ts';
import type { ArtifactDigest, PatchDigest, ReleaseArtifacts, WakeWord } from '../manifest/types.ts';

interface Args {
  channel: string;
  daemonVersion: string;
  imageVersion: string;
  daemonBumped: boolean;
  imageBumped: boolean;
  size: number;
  sha256: string;
  url: string;
  bundlesPath: string;
  builtinWebapps: Record<string, string>;
  wakeword?: WakeWord;
  artifacts: ReleaseArtifacts;
}

const ARTIFACT_KEYS = ['daemon', 'daemon_zst', 'image_swu', 'image_zck', 'image_boot_zck'] as const;
type ArtifactKey = (typeof ARTIFACT_KEYS)[number];

const WAKEWORD_ARTIFACT_KEYS = ['runtime', 'model'] as const;
type WakewordArtifactKey = (typeof WAKEWORD_ARTIFACT_KEYS)[number];

function parseDigest(flag: string, value: string): ArtifactDigest {
  const colon = value.lastIndexOf(':');
  if (colon <= 0) throw new Error(`${flag} expects <size>:<sha256>, got "${value}"`);
  const size = parseInt(value.slice(0, colon), 10);
  const sha256 = value.slice(colon + 1);
  if (!Number.isFinite(size) || size < 0) throw new Error(`${flag}: invalid size in "${value}"`);
  if (!/^[a-f0-9]{64}$/.test(sha256)) throw new Error(`${flag}: invalid sha256 in "${value}"`);
  return { size, sha256 };
}

function parsePatchDigest(flag: string, value: string): PatchDigest {
  const parts = value.split(':');
  if (parts.length < 2 || parts.length > 3) {
    throw new Error(`${flag} expects <size>:<sha256>[:<source_sha256>], got "${value}"`);
  }
  const digest = parseDigest(flag, `${parts[0]}:${parts[1]}`);
  if (parts.length === 2) return digest;
  const source = parts[2]!;
  if (!/^[a-f0-9]{64}$/.test(source)) throw new Error(`${flag}: invalid source_sha256 in "${value}"`);
  return { ...digest, source_sha256: source };
}

function parseArgs(argv: string[]): Args {
  const out: Partial<Record<Exclude<keyof Args, 'builtinWebapps' | 'artifacts'>, string>> = {};
  const builtinWebapps: Record<string, string> = {};
  const artifacts: ReleaseArtifacts = {};
  const webappArtifacts: Record<string, ArtifactDigest> = {};
  const daemonPatches: Record<string, PatchDigest> = {};
  const wakewordArtifacts: Record<string, ArtifactDigest> = {};
  const trainedAgainst: Record<string, string> = {};
  let wakewordRuntime = '';
  let wakewordModel = '';
  for (let i = 0; i < argv.length; i++) {
    const flag = argv[i];
    const next = argv[i + 1];
    if (!next) continue;
    switch (flag) {
      case '--builtin-webapp': {
        const eq = next.indexOf('=');
        if (eq <= 0) throw new Error(`--builtin-webapp expects <slug>=<version>, got "${next}"`);
        builtinWebapps[next.slice(0, eq)] = next.slice(eq + 1);
        break;
      }
      case '--artifact': {
        const eq = next.indexOf('=');
        if (eq <= 0) throw new Error(`--artifact expects <key>=<size>:<sha256>, got "${next}"`);
        const key = next.slice(0, eq) as ArtifactKey;
        if (!ARTIFACT_KEYS.includes(key)) {
          throw new Error(`--artifact key must be one of ${ARTIFACT_KEYS.join(', ')}, got "${key}"`);
        }
        artifacts[key] = parseDigest('--artifact', next.slice(eq + 1));
        break;
      }
      case '--webapp-artifact': {
        const eq = next.indexOf('=');
        if (eq <= 0) throw new Error(`--webapp-artifact expects <slug>=<size>:<sha256>, got "${next}"`);
        webappArtifacts[next.slice(0, eq)] = parseDigest('--webapp-artifact', next.slice(eq + 1));
        break;
      }
      case '--daemon-patch': {
        const eq = next.indexOf('=');
        if (eq <= 0) {
          throw new Error(`--daemon-patch expects <from-version>=<size>:<sha256>[:<source_sha256>], got "${next}"`);
        }
        daemonPatches[next.slice(0, eq)] = parsePatchDigest('--daemon-patch', next.slice(eq + 1));
        break;
      }
      case '--wakeword-artifact': {
        const eq = next.indexOf('=');
        if (eq <= 0) throw new Error(`--wakeword-artifact expects <key>=<size>:<sha256>, got "${next}"`);
        const key = next.slice(0, eq) as WakewordArtifactKey;
        if (!WAKEWORD_ARTIFACT_KEYS.includes(key)) {
          throw new Error(`--wakeword-artifact key must be one of ${WAKEWORD_ARTIFACT_KEYS.join(', ')}, got "${key}"`);
        }
        wakewordArtifacts[key] = parseDigest('--wakeword-artifact', next.slice(eq + 1));
        break;
      }
      case '--wakeword-trained-against': {
        const eq = next.indexOf('=');
        if (eq <= 0) {
          throw new Error(`--wakeword-trained-against expects <model-version>=<runtime-version>, got "${next}"`);
        }
        trainedAgainst[next.slice(0, eq)] = next.slice(eq + 1);
        break;
      }
      case '--wakeword-runtime':
        wakewordRuntime = next;
        break;
      case '--wakeword-model':
        wakewordModel = next;
        break;
      case '--channel':
        out.channel = next;
        break;
      case '--daemon-version':
        out.daemonVersion = next;
        break;
      case '--image-version':
        out.imageVersion = next;
        break;
      case '--daemon-bumped':
        out.daemonBumped = next;
        break;
      case '--image-bumped':
        out.imageBumped = next;
        break;
      case '--size':
        out.size = next;
        break;
      case '--sha256':
        out.sha256 = next;
        break;
      case '--url':
        out.url = next;
        break;
      case '--bundles-path':
        out.bundlesPath = next;
        break;
    }
  }
  for (const k of ['channel', 'daemonVersion', 'imageVersion', 'size', 'sha256', 'url'] as const) {
    if (!out[k]) throw new Error(`missing --${k.replace(/[A-Z]/g, c => `-${c.toLowerCase()}`)}`);
  }
  if (Object.keys(webappArtifacts).length > 0) artifacts.webapps = webappArtifacts;
  if (Object.keys(daemonPatches).length > 0) artifacts.daemon_patches = daemonPatches;
  if (Object.keys(wakewordArtifacts).length > 0) artifacts.wakeword = wakewordArtifacts;

  if (Boolean(wakewordRuntime) !== Boolean(wakewordModel)) {
    throw new Error('--wakeword-runtime and --wakeword-model must be given together');
  }
  const wakeword: WakeWord | undefined = wakewordModel
    ? {
        runtime: wakewordRuntime,
        model: wakewordModel,
        ...(Object.keys(trainedAgainst).length > 0 ? { model_trained_against: trainedAgainst } : {}),
      }
    : undefined;
  if (!wakeword && Object.keys(trainedAgainst).length > 0) {
    throw new Error('--wakeword-trained-against needs --wakeword-runtime and --wakeword-model');
  }

  return {
    channel: out.channel!,
    daemonVersion: out.daemonVersion!,
    imageVersion: out.imageVersion!,
    daemonBumped: out.daemonBumped === 'true',
    imageBumped: out.imageBumped === 'true',
    size: parseInt(out.size!, 10),
    sha256: out.sha256!,
    url: out.url!,
    bundlesPath: out.bundlesPath ?? resolve(import.meta.dirname, '..', 'manifest', 'bundles.yaml'),
    builtinWebapps,
    wakeword,
    artifacts,
  };
}

const args = parseArgs(process.argv.slice(2));
const raw = await readFile(args.bundlesPath, 'utf-8');
const doc = parse(raw) as { bundles?: Record<string, unknown>[] };
const bundles = doc.bundles ?? [];

const newEntry: Record<string, unknown> = {
  daemonVersion: args.daemonVersion,
  imageVersion: args.imageVersion,
  channel: args.channel,
  releasedAt: new Date().toISOString().replace(/\.\d{3}Z$/, 'Z'),
  daemonBumped: args.daemonBumped,
  imageBumped: args.imageBumped,
  ...(Object.keys(args.builtinWebapps).length > 0 ? { builtinWebapps: args.builtinWebapps } : {}),
  ...(args.wakeword ? { wakeword: args.wakeword } : {}),
  ...(Object.keys(args.artifacts).length > 0 ? { artifacts: args.artifacts } : {}),
  download: { url: args.url, size: args.size, sha256: args.sha256 },
};

const newComposite = composeVersion(args.daemonVersion, args.imageVersion);
const replaceIdx = bundles.findIndex(
  b =>
    b['channel'] === args.channel &&
    b['daemonVersion'] === args.daemonVersion &&
    b['imageVersion'] === args.imageVersion,
);

if (replaceIdx >= 0) {
  bundles[replaceIdx] = newEntry;
} else {
  bundles.push(newEntry);
}

const updated = { bundles };
await writeFile(args.bundlesPath, stringify(updated, { indent: 2 }));
console.log(`updated bundles.yaml: ${replaceIdx >= 0 ? 'replaced' : 'added'} ${newComposite} on ${args.channel}`);
