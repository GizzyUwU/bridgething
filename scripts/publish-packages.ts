#!/usr/bin/env bun
// aligns the publishable TS packages to the daemon (cargo workspace) version, builds them, and publishes.
// dry run by default; pass --publish to actually push to npm.

import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const ROOT = join(import.meta.dir, '..');
const doPublish = process.argv.includes('--publish');

type Pkg = { name: string; dir: string; scoped: boolean };

const PACKAGES: Pkg[] = [
  { name: '@bridgething/lib', dir: 'crates/lib', scoped: true },
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

async function run(cmd: string, args: string[], cwd: string): Promise<void> {
  console.log(`\n$ ${cmd} ${args.join(' ')}  (cwd: ${cwd.replace(ROOT, '.')})`);
  const proc = Bun.spawn([cmd, ...args], { cwd, stdout: 'inherit', stderr: 'inherit', stdin: 'inherit' });
  const code = await proc.exited;
  if (code !== 0) {
    console.error(`\ncommand failed (exit ${code}): ${cmd} ${args.join(' ')}`);
    process.exit(code);
  }
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

const filters = PACKAGES.flatMap((p) => ['--filter', p.name]);
await run('bunx', ['turbo', 'run', 'build', ...filters], ROOT);

for (const p of PACKAGES) {
  if (doPublish) {
    const args = ['publish'];
    if (p.scoped) args.push('--access', 'public');
    await run('bun', args, join(ROOT, p.dir));
  } else {
    // pack, not publish --dry-run: the latter still demands npm auth just to preview.
    await run('bun', ['pm', 'pack', '--dry-run'], join(ROOT, p.dir));
  }
}

console.log(
  doPublish
    ? `\npublished ${PACKAGES.map((p) => p.name).join(', ')} at ${version}`
    : `\ndry run complete. re-run with --publish to publish ${PACKAGES.map((p) => p.name).join(', ')} at ${version}`,
);
