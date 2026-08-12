#!/usr/bin/env node
import { DeliveryClient, parseCompositeVersion, type UpdateEvent } from '@bridgething/core-node';
import type { BridgeThingMeta } from '@bridgething/lib';

type Args = {
  root: string;
  channel: string | null;
  host: string;
  cacheDir: string | null;
  version: string | null;
};

type DiscoverManifest = {
  channels: Record<string, { latest: string }>;
  releases: Record<string, { yanked: string | null; deprecated: boolean } | undefined>;
};

const DEFAULT_ROOT = 'https://ota.bridgething.com';
const DEFAULT_HOST = 'ws://bridgething.local:8892/';

function parseArgs(argv: string[]): Args {
  const args: Args = { root: DEFAULT_ROOT, channel: null, host: DEFAULT_HOST, cacheDir: null, version: null };

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--root') args.root = requireValue(argv, ++i, arg);
    else if (arg === '--channel') args.channel = requireValue(argv, ++i, arg);
    else if (arg === '--host') args.host = requireValue(argv, ++i, arg);
    else if (arg === '--cache-dir') args.cacheDir = requireValue(argv, ++i, arg);
    else if (arg === '--version') args.version = requireValue(argv, ++i, arg);
    else if (arg === '--help' || arg === '-h') {
      printHelp();
      process.exit(0);
    } else {
      console.error(`unknown argument: ${arg}`);
      printHelp();
      process.exit(1);
    }
  }
  args.root = args.root.replace(/\/$/, '');
  return args;
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

Updates a Car Thing over its network gateway (USB-gadget by default) to a release
on a channel, per the discover manifest.

Options:
  --root <url>        Manifest root URL. Default ${DEFAULT_ROOT}.
  --channel <name>    Channel to track. Defaults to the channel the device reports.
  --host <ws-url>     Daemon network gateway URL. Default ${DEFAULT_HOST}.
  --cache-dir <path>  Artifact download cache. Defaults to a directory under the OS tmpdir.
  --version <ver>     Composite version to install. Defaults to the channel's latest.
`);
}

async function main(): Promise<void> {
  const args = parseArgs(process.argv.slice(2));

  console.log(`connecting to ${args.host} ...`);
  const client = await DeliveryClient.connect(args.host, {
    appName: 'bridgething-updater',
    cacheDir: args.cacheDir ?? undefined,
  });

  const meta = (await client.meta()) as BridgeThingMeta | null;
  if (!meta) fail('the device connected but never announced its version');
  console.log(
    `connected: ${meta.modelName} (${client.deviceId()}) - daemon ${meta.appVersion}, image ${meta.imageVersion} (${meta.imageVariant}/${meta.channel})`,
  );

  const channelName = args.channel ?? meta.channel;
  console.log(`fetching manifest from ${args.root} (channel ${channelName}) ...`);
  const manifest = (await client.discoverManifest(args.root)) as DiscoverManifest;
  const channel = manifest.channels[channelName];
  if (!channel) fail(`channel '${channelName}' is not in the manifest`);

  const version = args.version ?? channel.latest;
  const release = manifest.releases[version];
  if (release && (release.yanked !== null || release.deprecated)) {
    fail(`${version} is ${release.yanked !== null ? 'yanked' : 'deprecated'}; refusing to install`);
  }

  const target = parseCompositeVersion(version);
  if (!target) fail(`'${version}' is not a composite version`);
  if (meta.appVersion === target.daemon && meta.imageVersion === target.image) {
    console.log('already up to date.');
    process.exit(0);
  }

  console.log(`applying ${version} ...`);
  const watching = watch(client);
  await client.applyVersion(channelName, version, args.root);
  process.exit((await watching) ? 0 : 1);
}

function watch(client: DeliveryClient): Promise<boolean> {
  return (async () => {
    for (;;) {
      const event: UpdateEvent = await client.nextEvent();
      switch (event.kind) {
        case 'planned':
          console.log(`  plan: ${(event.steps ?? []).map(step => step.label).join(' -> ')}`);
          break;
        case 'progress':
          if (event.phase) console.log(`  ${describe(event.phase)}`);
          break;
        case 'updated':
          console.log(`update finished: ${event.version ?? ''}`.trimEnd());
          return true;
        case 'failed':
          console.error(`update failed: ${event.reason ?? 'no reason given'}`);
          return false;
        case 'lagged':
          console.log('  (progress entries were dropped)');
          break;
      }
    }
  })();
}

function describe(phase: NonNullable<UpdateEvent['phase']>): string {
  const head = [phase.kind, phase.asset].filter(Boolean).join(' ');
  const done = phase.received ?? phase.sent;
  if (done !== undefined && phase.total) return `${head} ${Math.floor((done * 100) / phase.total)}%`;
  if (phase.writePercent !== undefined) return `${head} ${phase.writePercent}%`;
  if (phase.reason) return `${head}: ${phase.reason}`;
  return head;
}

function fail(message: string): never {
  console.error(message);
  process.exit(1);
}

main().catch(err => {
  console.error(err instanceof Error ? (err.stack ?? err.message) : err);
  process.exit(1);
});
