#!/usr/bin/env node
//! `create-bridgething` — copy the bundled template into a new directory,
//! substitute the project name, print next steps. Opinionated stack:
//! React 19 + Vite + Tailwind v4 + TypeScript strict, plus
//! `@bridgething/client` preinstalled.

import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { v7 as uuidv7 } from 'uuid';

const __dirname = dirname(fileURLToPath(import.meta.url));
const TEMPLATE_DIR = resolve(__dirname, '..', 'template');

type Args = {
  target: string;
  install: boolean;
  git: boolean;
};

function parseArgs(argv: string[]): Args {
  const positional: string[] = [];
  let install = true;
  let git = true;
  for (const arg of argv) {
    if (arg === '--no-install') install = false;
    else if (arg === '--no-git') git = false;
    else if (arg === '--help' || arg === '-h') {
      printHelp();
      process.exit(0);
    } else if (arg.startsWith('-')) {
      console.error(`unknown flag: ${arg}`);
      printHelp();
      process.exit(1);
    } else {
      positional.push(arg);
    }
  }
  if (positional.length !== 1) {
    printHelp();
    process.exit(1);
  }
  return { target: positional[0], install, git };
}

function printHelp(): void {
  console.log(`Usage: create-bridgething <target-dir> [--no-install] [--no-git]

Scaffold a new bridgething webapp at <target-dir>.

Options:
  --no-install  Skip 'bun install' after copying.
  --no-git      Skip 'git init' after copying.
`);
}

type Substitutions = {
  projectName: string;
  webappUuid: string;
};

function copyTemplate(src: string, dest: string, subs: Substitutions): void {
  for (const entry of readdirSync(src)) {
    const srcPath = join(src, entry);
    const destPath = join(dest, entry === '_gitignore' ? '.gitignore' : entry);
    const stat = statSync(srcPath);
    if (stat.isDirectory()) {
      mkdirSync(destPath, { recursive: true });
      copyTemplate(srcPath, destPath, subs);
    } else {
      const raw = readFileSync(srcPath, 'utf8');
      const substituted = raw
        .replace(/__PROJECT_NAME__/g, subs.projectName)
        .replace(/__WEBAPP_UUID__/g, subs.webappUuid);
      writeFileSync(destPath, substituted);
    }
  }
}

function run(cmd: string, args: string[], cwd: string): boolean {
  const result = spawnSync(cmd, args, { cwd, stdio: 'inherit' });
  return result.status === 0;
}

function main(): void {
  const args = parseArgs(process.argv.slice(2));
  const target = resolve(process.cwd(), args.target);
  const projectName = args.target.replace(/^.*[\\/]/, '');

  if (existsSync(target)) {
    const entries = readdirSync(target);
    if (entries.length > 0) {
      console.error(`error: ${target} exists and is not empty`);
      process.exit(1);
    }
  } else {
    mkdirSync(target, { recursive: true });
  }

  const webappUuid = uuidv7();
  console.log(`scaffolding ${projectName} (${webappUuid}) in ${target}`);
  copyTemplate(TEMPLATE_DIR, target, { projectName, webappUuid });
  console.log('  ✓ template copied');

  if (args.git) {
    if (run('git', ['init', '--quiet'], target)) {
      console.log('  ✓ git initialized');
    } else {
      console.warn('  ! git init failed (skipping)');
    }
  }

  if (args.install) {
    console.log('  installing dependencies with bun...');
    if (!run('bun', ['install'], target)) {
      console.warn('  ! bun install failed; install manually with `bun install`');
    } else {
      console.log('  ✓ dependencies installed');
    }
  }

  console.log(`
Done! Next steps:

  cd ${args.target}
${args.install ? '' : '  bun install\n'}  bun run dev          # local dev server (http://localhost:5173/)
  bun run build        # production bundle into dist/
  bun run push <ip>    # rsync dist/ to /var/bridgething/webapps/${projectName}/ on the device

The starter App connects to ws://127.0.0.1:8891/ in production (the
on-device daemon), and to ws://<device-ip>:8891/ in dev when you set
VITE_BRIDGETHING_URL.
`);
}

main();
