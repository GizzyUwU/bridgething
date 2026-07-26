#!/usr/bin/env node
import { mkdir, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import type { BridgeThingMeta } from '@bridgething/gateway';
import { BridgethingGateway } from '@bridgething/gateway';

import { OtaDriver, type OtaProgressSnapshot } from './driver.js';
import {
  type OtaManifestRelease,
  daemonPatchUrl,
  fetchManifest,
  imageVariantForChannel,
  otaArtifactUrls,
  parseCompositeVersion,
} from './manifest.js';
import { fileArtifactSource } from './node.js';
import { NetworkAdapter } from './websocket.js';

type Args = {
  root: string;
  channel: string | null;
  host: string;
  cacheDir: string;
  daemonOnly: boolean;
};

const DEFAULT_ROOT = 'https://ota.bridgething.com';
const DEFAULT_HOST = 'ws://bridgething.local:8892/';
const CONNECT_TIMEOUT_MS = 15_000;

function parseArgs(argv: string[]): Args {
  let root = DEFAULT_ROOT;
  let channel: string | null = null;
  let host = DEFAULT_HOST;
  let cacheDir = join(tmpdir(), 'bridgething-updater');
  let daemonOnly = false;

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--root') root = requireValue(argv, ++i, arg);
    else if (arg === '--channel') channel = requireValue(argv, ++i, arg);
    else if (arg === '--host') host = requireValue(argv, ++i, arg);
    else if (arg === '--cache-dir') cacheDir = requireValue(argv, ++i, arg);
    else if (arg === '--daemon-only') daemonOnly = true;
    else if (arg === '--help' || arg === '-h') {
      printHelp();
      process.exit(0);
    } else {
      console.error(`unknown argument: ${arg}`);
      printHelp();
      process.exit(1);
    }
  }
  return { root: root.replace(/\/$/, ''), channel, host, cacheDir, daemonOnly };
}

function requireValue(argv: string[], idx: number, flag: string): string {
  const value = argv[idx];
  if (value === undefined) {
    console.error(`${flag} requires a value`);
    process.exit(1);
  }
  return value;
}

function printHelp(): void {
  console.log(`Usage: bridgething-updater [options]

Updates a Car Thing over its network gateway (USB-gadget by default) to the latest
release on a channel, per the discover manifest.

Options:
  --root <url>       Manifest root URL. Default ${DEFAULT_ROOT}.
  --channel <name>    Channel to track. Defaults to the channel the device reports.
  --host <ws-url>     Daemon network gateway URL. Default ${DEFAULT_HOST}.
  --cache-dir <path>  Artifact download cache. Default a bridgething-updater dir under the OS tmpdir.
  --daemon-only       Skip the image OTA even if the manifest's image half changed.
`);
}

async function main(): Promise<void> {
  const args = parseArgs(process.argv.slice(2));
  await mkdir(args.cacheDir, { recursive: true });

  console.log(`connecting to ${args.host} ...`);
  const adapter = new NetworkAdapter({ discovery: args.host });
  const gateway = new BridgethingGateway(adapter);
  await gateway.start();

  const { deviceId, meta } = await waitForDevice(gateway);
  console.log(
    `connected: ${meta.modelName} (${deviceId}) - daemon ${meta.appVersion}, image ${meta.imageVersion} (${meta.imageVariant}/${meta.channel})`,
  );

  const channelName = args.channel ?? meta.channel;
  console.log(`fetching manifest from ${args.root} (channel ${channelName}) ...`);
  const manifest = await fetchManifest(args.root);
  const channel = manifest.channels[channelName];
  if (!channel) {
    fail(`channel '${channelName}' not present in manifest`);
  }
  const composite = parseCompositeVersion(channel.latest);
  if (!composite) {
    fail(`channel.latest '${channel.latest}' is not a composite version`);
  }
  const release = manifest.releases[channel.latest];
  if (release && (release.yanked !== null || release.deprecated)) {
    fail(
      `latest release ${channel.latest} is ${release.yanked !== null ? 'yanked' : 'deprecated'}; refusing to install`,
    );
  }

  const urls = otaArtifactUrls({
    rootURL: args.root,
    channel: channelName,
    daemonVersion: composite.daemon,
    imageVersion: composite.image,
    imageVariant: imageVariantForChannel(channelName),
  });

  const driver = new OtaDriver(gateway, deviceId);
  let ok = true;
  try {
    if (!args.daemonOnly && meta.imageVersion !== composite.image) {
      ok = await runImagePush(driver, args, urls, `${channelName}-${composite.image}`);
    } else if (meta.appVersion !== composite.daemon) {
      ok = await runDaemonPush(driver, args, {
        fullUrl: urls.daemonBinary,
        zstUrl: urls.daemonBinaryZst,
        toVersion: composite.daemon,
        fromVersion: meta.appVersion,
        fromSha256: meta.daemonSha256 ?? null,
        channel: channelName,
        release,
      });
    } else {
      console.log('already up to date.');
    }
  } finally {
    driver.close();
  }

  await gateway.stop();
  process.exit(ok ? 0 : 1);
}

async function runImagePush(
  driver: OtaDriver,
  args: Args,
  urls: ReturnType<typeof otaArtifactUrls>,
  tag: string,
): Promise<boolean> {
  console.log('downloading image artifacts ...');
  const swuPath = await downloadIfNeeded(urls.imageSwu, args.cacheDir, `image-${tag}.swu`);
  const zckPath = await downloadIfNeeded(urls.imageZck, args.cacheDir, `image-${tag}.zck`);
  const bootZckPath = await downloadIfNeeded(urls.imageBootZck, args.cacheDir, `image-${tag}-boot.zck`);

  const source = await fileArtifactSource(swuPath);
  const zcks = new Map([
    ['system.img.zck', await fileArtifactSource(zckPath)],
    ['boot.vfat.zck', await fileArtifactSource(bootZckPath)],
  ]);

  console.log('pushing image OTA ...');
  const snapshot = await driver.pushImage({ source, zcks, updateUrlBase: args.root, onProgress: logProgress });
  return reportOutcome(snapshot);
}

async function runDaemonPush(
  driver: OtaDriver,
  args: Args,
  opts: {
    fullUrl: string;
    zstUrl: string;
    toVersion: string;
    fromVersion: string;
    fromSha256: string | null;
    channel: string;
    release: OtaManifestRelease | undefined;
  },
): Promise<boolean> {
  const artifacts = opts.release?.artifacts;
  const patchDigest = artifacts?.daemon_patches?.[opts.fromVersion];
  const daemonDigest = artifacts?.daemon;

  const sourceMatches =
    !patchDigest?.source_sha256 || !opts.fromSha256 || patchDigest.source_sha256 === opts.fromSha256;
  if (patchDigest && !sourceMatches) {
    console.log(
      `skipping daemon delta: device binary ${opts.fromSha256?.slice(0, 12)} is not the published ` +
        `${opts.fromVersion} (${patchDigest.source_sha256?.slice(0, 12)}); full binary instead.`,
    );
  }

  if (patchDigest && daemonDigest && sourceMatches) {
    console.log(`downloading daemon delta ${opts.fromVersion} -> ${opts.toVersion} ...`);
    const patchUrl = daemonPatchUrl({
      rootURL: args.root,
      channel: opts.channel,
      toVersion: opts.toVersion,
      fromVersion: opts.fromVersion,
    });
    const patchPath = await downloadIfNeeded(
      patchUrl,
      args.cacheDir,
      `daemon-${opts.toVersion}-from-${opts.fromVersion}.patch`,
    );
    const patchSource = await fileArtifactSource(patchPath);

    console.log('pushing daemon delta OTA ...');
    const snapshot = await driver.pushDaemon(patchSource, logProgress, {
      algorithm: 'zstdPatchFrom',
      resultSha256: daemonDigest.sha256,
      resultSize: daemonDigest.size,
      sourceSha256: patchDigest.source_sha256 ?? null,
    });
    if (snapshot.phase !== 'failed') return reportOutcome(snapshot);
    console.log(`daemon delta failed (${snapshot.reason}); falling back to full binary ...`);
  }

  const zst = artifacts?.daemon_zst;
  if (zst && daemonDigest) {
    console.log(`downloading compressed daemon ${opts.toVersion} ...`);
    const zstPath = await downloadIfNeeded(opts.zstUrl, args.cacheDir, `daemon-${opts.toVersion}.zst`);
    const zstSource = await fileArtifactSource(zstPath);
    console.log('pushing compressed daemon OTA ...');
    const snapshot = await driver.pushDaemon(zstSource, logProgress, {
      algorithm: 'zstd',
      resultSha256: daemonDigest.sha256,
      resultSize: daemonDigest.size,
      sourceSha256: null,
    });
    if (snapshot.phase !== 'failed') return reportOutcome(snapshot);
    console.log(`compressed daemon push failed (${snapshot.reason}); falling back to the raw binary ...`);
  }

  console.log(`downloading daemon ${opts.toVersion} ...`);
  const path = await downloadIfNeeded(opts.fullUrl, args.cacheDir, `daemon-${opts.toVersion}`);
  const source = await fileArtifactSource(path);

  console.log('pushing daemon OTA ...');
  const snapshot = await driver.pushDaemon(source, logProgress);
  return reportOutcome(snapshot);
}

function logProgress(snapshot: OtaProgressSnapshot): void {
  switch (snapshot.phase) {
    case 'streaming':
      console.log(`  streaming: ${snapshot.percent}%`);
      break;
    case 'applying':
      console.log(`  ${snapshot.otaPhase}: ${snapshot.percent}%`);
      break;
    case 'staged':
      console.log('  staged, activating ...');
      break;
    case 'completed':
      console.log('  completed.');
      break;
    case 'failed':
      console.error(`  failed: ${snapshot.reason}`);
      break;
  }
}

function reportOutcome(snapshot: OtaProgressSnapshot): boolean {
  if (snapshot.phase === 'failed') {
    console.error(`update failed: ${snapshot.reason}`);
    return false;
  }
  console.log('update finished.');
  return true;
}

async function waitForDevice(gateway: BridgethingGateway): Promise<{ deviceId: string; meta: BridgeThingMeta }> {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      unsubscribe();
      reject(new Error(`no device announced itself within ${CONNECT_TIMEOUT_MS / 1000}s`));
    }, CONNECT_TIMEOUT_MS);
    const unsubscribe = gateway.on(event => {
      if (event.type !== 'message') return;
      if (event.message.data.type !== 'version') return;
      clearTimeout(timeout);
      unsubscribe();
      resolve({ deviceId: event.deviceId, meta: event.message.data.data });
    });
  });
}

async function downloadIfNeeded(url: string, cacheDir: string, filename: string): Promise<string> {
  const target = join(cacheDir, filename);
  const existing = await stat(target).catch(() => null);
  if (existing && existing.size > 0) return target;

  const response = await fetch(url);
  if (!response.ok || !response.body) {
    throw new Error(`download failed: HTTP ${response.status} for ${url}`);
  }
  const bytes = new Uint8Array(await response.arrayBuffer());
  await writeFile(target, bytes);
  return target;
}

function fail(message: string): never {
  console.error(message);
  process.exit(1);
}

main().catch(err => {
  console.error(err instanceof Error ? (err.stack ?? err.message) : err);
  process.exit(1);
});
