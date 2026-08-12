#!/usr/bin/env bun
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { cp, mkdir, mkdtemp, readdir, readFile, rm, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';

interface Args {
  imageZip: string;
  daemonBin: string;
  imageVersion: string;
  daemonVersion: string;
  channel: string;
  output: string;
}

function parseArgs(argv: string[]): Args {
  const args: Partial<Args> = {};
  for (let i = 0; i < argv.length; i++) {
    const flag = argv[i];
    const next = argv[i + 1];
    switch (flag) {
      case '--image-zip':
        args.imageZip = next;
        i++;
        break;
      case '--daemon-bin':
        args.daemonBin = next;
        i++;
        break;
      case '--image-version':
        args.imageVersion = next;
        i++;
        break;
      case '--daemon-version':
        args.daemonVersion = next;
        i++;
        break;
      case '--channel':
        args.channel = next;
        i++;
        break;
      case '--output':
        args.output = next;
        i++;
        break;
      case '--help':
      case '-h':
        printHelpAndExit(0);
        break;
      default:
        console.error(`unknown argument: ${flag}`);
        printHelpAndExit(2);
    }
  }
  for (const required of ['imageZip', 'daemonBin', 'imageVersion', 'daemonVersion', 'channel', 'output'] as const) {
    if (!args[required]) {
      console.error(`missing required argument --${required.replace(/[A-Z]/g, c => `-${c.toLowerCase()}`)}`);
      printHelpAndExit(2);
    }
  }
  return args as Args;
}

function printHelpAndExit(code: number): never {
  console.log(
    [
      'Usage: bun run scripts/bundle-zip.ts [options]',
      '',
      'Required:',
      '  --image-zip <path>       yocto-produced flashthing.zip for the image release',
      '  --daemon-bin <path>      aarch64 daemon binary to swap into bandaid.ext4',
      '  --image-version <ver>    e.g. 2026.05.0 (sanity-check + summary)',
      '  --daemon-version <ver>   e.g. 0.1.0 (sanity-check + summary)',
      '  --channel <slug>         stable | dev',
      '  --output <path>          output .zip path',
    ].join('\n'),
  );
  process.exit(code);
}

function run(cmd: string, args: string[], cwd?: string): void {
  const r = spawnSync(cmd, args, { stdio: ['inherit', 2, 'inherit'], cwd });
  if (r.status !== 0) {
    throw new Error(`${cmd} ${args.join(' ')} exited with status ${r.status}`);
  }
}

async function sha256(path: string): Promise<string> {
  return await new Promise<string>((resolveP, rejectP) => {
    const hash = createHash('sha256');
    const stream = createReadStream(path);
    stream.on('data', chunk => hash.update(chunk));
    stream.on('end', () => resolveP(hash.digest('hex')));
    stream.on('error', rejectP);
  });
}

async function swapDaemonInBandaid(bandaidPath: string, daemonBin: string): Promise<void> {
  const stage = await mkdtemp(join(tmpdir(), 'bt-bandaid-'));
  try {
    run('debugfs', ['-R', `rdump / ${stage}`, bandaidPath]);

    const daemonInBandaid = join(stage, 'bridgething', 'daemon', 'bridgething.current');
    await mkdir(dirname(daemonInBandaid), { recursive: true });
    await cp(daemonBin, daemonInBandaid);

    const sizeBytes = (await stat(bandaidPath)).size;
    await rm(bandaidPath);
    run('mke2fs', [
      '-q',
      '-t',
      'ext4',
      '-L',
      'bandaid',
      '-d',
      stage,
      '-E',
      'root_owner=0:0',
      '-O',
      '^has_journal',
      bandaidPath,
      String(Math.floor(sizeBytes / 1024)),
    ]);
  } finally {
    await rm(stage, { recursive: true, force: true });
  }
}

export interface FlashMeta {
  name: string;
  version: string;
  description: string;
  metadataVersion: number;
  steps: unknown[];
  [key: string]: unknown;
}

export function composeMeta(
  meta: FlashMeta,
  args: Pick<Args, 'daemonVersion' | 'imageVersion' | 'channel'>,
): FlashMeta {
  if (meta.metadataVersion !== 2) {
    throw new Error(`meta.json is metadataVersion ${meta.metadataVersion}; the composer only knows v2`);
  }
  if (!Array.isArray(meta.steps) || meta.steps.length === 0) {
    throw new Error('meta.json has no steps; the yocto zip is not a flashable package');
  }
  return {
    ...meta,
    name: args.channel === 'dev' ? 'bridgething-dev' : 'bridgething',
    version: `${args.daemonVersion}+image.${args.imageVersion}`,
    description: `bridgething ${args.daemonVersion} on image ${args.imageVersion} (${args.channel})`,
  };
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const stage = await mkdtemp(join(tmpdir(), 'bt-bundle-'));
  try {
    console.error(`[1/4] unpacking ${basename(args.imageZip)}`);
    run('unzip', ['-q', resolve(args.imageZip), '-d', stage]);

    const bandaid = join(stage, 'bandaid.ext4');
    if (!(await fileExists(bandaid))) {
      throw new Error(
        `bandaid.ext4 not found in ${basename(args.imageZip)}; the yocto image is not built with MAINLINE_FLASHTHING_WITH_BANDAID = "1"`,
      );
    }

    console.error(`[2/5] swapping daemon binary into bandaid.ext4`);
    await swapDaemonInBandaid(bandaid, args.daemonBin);

    console.error(`[3/5] restamping meta.json`);
    const metaPath = join(stage, 'meta.json');
    if (!(await fileExists(metaPath))) {
      throw new Error(`meta.json not found in ${basename(args.imageZip)}`);
    }
    const meta = composeMeta(JSON.parse(await readFile(metaPath, 'utf-8')) as FlashMeta, args);
    await writeFile(metaPath, `${JSON.stringify(meta, null, 2)}\n`);

    console.error(`[4/5] composing ${basename(args.output)}`);
    const out = resolve(args.output);
    await mkdir(dirname(out), { recursive: true });
    await rm(out, { force: true });
    const entries = (await readdir(stage)).filter(e => !e.startsWith('.'));
    run('zip', ['-q', '-X', out, ...entries], stage);

    console.error(`[5/5] hashing`);
    const size = (await stat(out)).size;
    const digest = await sha256(out);

    const summary = {
      output: out,
      size,
      sha256: digest,
      version: `${args.daemonVersion}+image.${args.imageVersion}`,
      channel: args.channel,
    };
    console.log(JSON.stringify(summary, null, 2));
  } finally {
    await rm(stage, { recursive: true, force: true });
  }
}

async function fileExists(path: string): Promise<boolean> {
  try {
    await stat(path);
    return true;
  } catch {
    return false;
  }
}

if (import.meta.main) await main();
