#!/usr/bin/env bun
// aligns the publishable TS packages to the daemon (cargo workspace) version, builds them, and publishes.
// dry run by default; pass --publish to actually push to npm.

import { existsSync, readFileSync, unlinkSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const ROOT = join(import.meta.dir, '..');
const doPublish = process.argv.includes('--publish');

type Pkg = { name: string; dir: string; scoped: boolean };

const PACKAGES: Pkg[] = [
  { name: '@bridgething/lib', dir: 'crates/lib', scoped: true },
  { name: '@bridgething/gateway', dir: 'packages/gateway/typescript', scoped: true },
  { name: '@bridgething/adapter-network', dir: 'packages/adapter-network', scoped: true },
  { name: '@bridgething/updater', dir: 'packages/updater', scoped: true },
  { name: '@bridgething/client', dir: 'packages/client-ts', scoped: true },
  { name: 'create-bridgething', dir: 'packages/create-bridgething', scoped: false },
];

// the create-bridgething template ships this dep pinned; keep it in lockstep so scaffolds pull the matching client.
const TEMPLATE_MANIFEST = 'packages/create-bridgething/template/package.json';
const TEMPLATE_DEP = '@bridgething/client';

function daemonVersion(): string {
  const cargo = readFileSync(join(ROOT, 'Cargo.toml'), 'utf8');
  const version = cargo.match(/^\s*version\s*=\s*["']([^"']+)["']/m)?.[1];
  if (!version) {
    console.error('could not read version from Cargo.toml [workspace.package]');
    process.exit(1);
  }
  return version;
}

function setVersion(manifestPath: string, version: string): string | null {
  const full = join(ROOT, manifestPath);
  const src = readFileSync(full, 'utf8');
  const current = src.match(/"version"\s*:\s*"([^"]+)"/)?.[1] ?? null;
  const next = src.replace(/("version"\s*:\s*")[^"]+(")/, `$1${version}$2`);
  if (next !== src) writeFileSync(full, next);
  return current;
}

function setTemplateDep(version: string): string | null {
  const full = join(ROOT, TEMPLATE_MANIFEST);
  const src = readFileSync(full, 'utf8');
  const re = new RegExp(`("${TEMPLATE_DEP.replace('/', '\\/')}"\\s*:\\s*")([^"]+)(")`);
  const current = src.match(re)?.[2] ?? null;
  const next = src.replace(re, `$1^${version}$3`);
  if (next !== src) writeFileSync(full, next);
  return current;
}

function reEscape(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\/]/g, '\\$&');
}

// bun.lock caches each workspace member's version, and `workspace:*` resolves against that cache, not the live
// package.json. incremental `bun install` won't refresh it (only a full regen does, which churns every transitive dep),
// so patch the members we publish in place, in lockstep with their package.json bumps.
function patchLockfile(version: string): void {
  const full = join(ROOT, 'bun.lock');
  if (!existsSync(full)) return;
  let src = readFileSync(full, 'utf8');
  for (const p of PACKAGES) {
    const re = new RegExp(
      `("${reEscape(p.dir)}":\\s*\\{\\s*"name":\\s*"${reEscape(p.name)}",\\s*"version":\\s*")[^"]+(")`,
    );
    src = src.replace(re, `$1${version}$2`);
  }
  writeFileSync(full, src);
}

async function run(cmd: string, args: string[], cwd: string): Promise<void> {
  console.log(`\n$ ${cmd} ${args.join(' ')}  (cwd: ${cwd.replace(ROOT, '.')})`);
  const proc = Bun.spawn([cmd, ...args], { cwd, stdout: 'inherit', stderr: 'inherit', stdin: 'inherit' });
  const code = await proc.exited;
  if (code !== 0) {
    console.error(`\ncommand failed (exit ${code}): ${cmd} ${args.join(' ')}`);
    process.exit(code);
  }
}

async function capture(cmd: string, args: string[], cwd: string): Promise<string> {
  console.log(`\n$ ${cmd} ${args.join(' ')}  (cwd: ${cwd.replace(ROOT, '.')})`);
  const proc = Bun.spawn([cmd, ...args], { cwd, stdout: 'pipe', stderr: 'inherit', stdin: 'inherit' });
  const out = await new Response(proc.stdout).text();
  process.stdout.write(out);
  const code = await proc.exited;
  if (code !== 0) {
    console.error(`\ncommand failed (exit ${code}): ${cmd} ${args.join(' ')}`);
    process.exit(code);
  }
  return out;
}

const version = daemonVersion();
console.log(`daemon version: ${version}`);
console.log(doPublish ? 'mode: PUBLISH (real)\n' : 'mode: dry run (pass --publish to publish for real)\n');

console.log('aligning package versions:');
for (const p of PACKAGES) {
  const prev = setVersion(`${p.dir}/package.json`, version);
  console.log(`  ${p.name}: ${prev} -> ${version}`);
}
const prevDep = setTemplateDep(version);
console.log(`  template dep ${TEMPLATE_DEP}: ${prevDep ?? '(none)'} -> ^${version}`);
patchLockfile(version);
console.log('  bun.lock: workspace members synced');

const filters = PACKAGES.flatMap(p => ['--filter', p.name]);
await run('bunx', ['turbo', 'run', 'build', ...filters], ROOT);

// two-step per https://github.com/oven-sh/bun (bun publish can't read .npmrc auth since oct 2025):
//   bun pm pack resolves workspace:* to real versions, then npm publish uses npm's reliable .npmrc auth.
for (const p of PACKAGES) {
  const dir = join(ROOT, p.dir);
  if (!doPublish) {
    await run('bun', ['pm', 'pack', '--dry-run'], dir);
    continue;
  }
  const packOut = await capture('bun', ['pm', 'pack'], dir);
  const tarball = packOut
    .trim()
    .split('\n')
    .map(l => l.trim())
    .reverse()
    .find(l => l.endsWith('.tgz'));
  if (!tarball) {
    console.error(`could not find packed tarball name for ${p.name}`);
    process.exit(1);
  }
  const npmArgs = ['publish', tarball];
  if (p.scoped) npmArgs.push('--access', 'public');
  console.log(`\n$ npm ${npmArgs.join(' ')}  (cwd: ${dir.replace(ROOT, '.')})`);
  const proc = Bun.spawn(['npm', ...npmArgs], { cwd: dir, stdout: 'inherit', stderr: 'inherit', stdin: 'inherit' });
  const code = await proc.exited;
  const tgz = join(dir, tarball);
  if (existsSync(tgz)) unlinkSync(tgz);
  if (code !== 0) {
    console.error(`\nnpm publish failed (exit ${code}) for ${p.name}`);
    process.exit(code);
  }
}

console.log(
  doPublish
    ? `\npublished ${PACKAGES.map(p => p.name).join(', ')} at ${version}`
    : `\ndry run complete. re-run with --publish to publish ${PACKAGES.map(p => p.name).join(', ')} at ${version}`,
);
