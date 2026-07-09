#!/usr/bin/env node
//! `bridgething-updater` - manifest-driven CLI. Connects to a Car Thing over the daemon's
//! network gateway, resolves the target channel's `latest` composite version from the discover
//! manifest, downloads whatever artifacts are missing, and pushes daemon and/or image OTA.

import { mkdir, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import type { BridgeThingMeta } from '@bridgething/gateway';
import { BridgethingGateway } from '@bridgething/gateway';

import { OtaDriver, type OtaProgressSnapshot } from './driver';
import { fetchManifest, otaArtifactUrls, parseCompositeVersion } from './manifest';
import { fileArtifactSource } from './node';
import { NetworkAdapter } from './websocket';

type Args = {
  root: string;
  channel: string;
  host: string;
  cacheDir: string;
  daemonOnly: boolean;
};

const DEFAULT_ROOT = 'https://ota.bridgething.com';
const DEFAULT_HOST = 'ws://bridgething.local:8892/';
const CONNECT_TIMEOUT_MS = 15_000;

function parseArgs(argv: string[]): Args {
  let root = DEFAULT_ROOT;
  let channel = 'stable';
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
  --channel <name>    Channel to track. Default 'stable'.
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

  console.log(`fetching manifest from ${args.root} ...`);
  const manifest = await fetchManifest(args.root);
  const channel = manifest.channels[args.channel];
  if (!channel) {
    fail(`channel '${args.channel}' not present in manifest`);
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
    channel: args.channel,
    daemonVersion: composite.daemon,
    imageVersion: composite.image,
    imageVariant: meta.imageVariant,
  });

  const driver = new OtaDriver(gateway, deviceId);
  let ok = true;
  try {
    if (!args.daemonOnly && meta.imageVersion !== composite.image) {
      ok = await runImagePush(driver, args, urls);
    } else if (meta.appVersion !== composite.daemon) {
      ok = await runDaemonPush(driver, args, urls.daemonBinary, composite.daemon);
    } else {
      console.log('already up to date.');
    }
  } finally {
    driver.close();
  }

  await gateway.stop();
  process.exit(ok ? 0 : 1);
}

async function runImagePush(driver: OtaDriver, args: Args, urls: ReturnType<typeof otaArtifactUrls>): Promise<boolean> {
  console.log('downloading image artifacts ...');
  const swuPath = await downloadIfNeeded(urls.imageSwu, args.cacheDir, 'image.swu');
  const zckPath = await downloadIfNeeded(urls.imageZck, args.cacheDir, 'image.zck');
  const bootZckPath = await downloadIfNeeded(urls.imageBootZck, args.cacheDir, 'image-boot.zck');

  const source = await fileArtifactSource(swuPath);
  const zcks = new Map([
    ['system.img.zck', await fileArtifactSource(zckPath)],
    ['boot.vfat.zck', await fileArtifactSource(bootZckPath)],
  ]);

  console.log('pushing image OTA ...');
  const snapshot = await driver.pushImage({ source, zcks, updateUrlBase: args.root, onProgress: logProgress });
  return reportOutcome(snapshot);
}

async function runDaemonPush(driver: OtaDriver, args: Args, url: string, version: string): Promise<boolean> {
  console.log(`downloading daemon ${version} ...`);
  const path = await downloadIfNeeded(url, args.cacheDir, `daemon-${version}`);
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
