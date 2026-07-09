#!/usr/bin/env node
//! `create-bridgething` - copy the bundled template into a new directory,
//! substitute the project name, print next steps. Opinionated stack:
//! React 19 + Vite + Tailwind v4 + TypeScript strict, plus
//! `@bridgething/client` preinstalled.

import { spawnSync } from 'node:child_process';
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  statSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import { dirname, extname, join, resolve } from 'node:path';
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

const BINARY_EXT = new Set([
  '.ttf',
  '.otf',
  '.woff',
  '.woff2',
  '.png',
  '.jpg',
  '.jpeg',
  '.gif',
  '.webp',
  '.avif',
  '.ico',
  '.wasm',
]);

function copyTemplate(src: string, dest: string, subs: Substitutions): void {
  for (const entry of readdirSync(src)) {
    const srcPath = join(src, entry);
    const renamed = entry === '_gitignore' ? '.gitignore' : entry === '_claude' ? '.claude' : entry;
    const destPath = join(dest, renamed);
    const stat = statSync(srcPath);
    if (stat.isDirectory()) {
      mkdirSync(destPath, { recursive: true });
      copyTemplate(srcPath, destPath, subs);
    } else if (BINARY_EXT.has(extname(entry).toLowerCase())) {
      copyFileSync(srcPath, destPath);
    } else {
      const raw = readFileSync(srcPath, 'utf8');
      const substituted = raw
        .replace(/__PROJECT_NAME__/g, subs.projectName)
        .replace(/__WEBAPP_UUID__/g, subs.webappUuid);
      writeFileSync(destPath, substituted);
    }
  }
}

function copyDir(src: string, dest: string): void {
  mkdirSync(dest, { recursive: true });
  for (const entry of readdirSync(src)) {
    const srcPath = join(src, entry);
    const destPath = join(dest, entry);
    if (statSync(srcPath).isDirectory()) copyDir(srcPath, destPath);
    else copyFileSync(srcPath, destPath);
  }
}

function linkAgentAliases(target: string): void {
  try {
    symlinkSync('CLAUDE.md', join(target, 'AGENTS.md'));
  } catch {
    copyFileSync(join(target, 'CLAUDE.md'), join(target, 'AGENTS.md'));
  }
  mkdirSync(join(target, '.agents'), { recursive: true });
  try {
    symlinkSync(join('..', '.claude', 'skills'), join(target, '.agents', 'skills'), 'dir');
  } catch {
    copyDir(join(target, '.claude', 'skills'), join(target, '.agents', 'skills'));
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

  linkAgentAliases(target);
  console.log('  ✓ agent guides linked (CLAUDE.md, AGENTS.md, /bridgething skill)');

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

Open this folder with your coding agent (Claude Code, Codex, opencode, ...).
It reads CLAUDE.md / AGENTS.md and the /bridgething skill in .claude/skills
(mirrored at .agents/skills), which goes deep on the client API, running and
driving the app, and installing and sharing it.

  bun run build        # production bundle into dist/
  bun run push <addr>  # build + install onto a connected Car Thing (default bridgething.local)
  bun run share        # build first, then zip dist/ to hand to friends
  bun run update       # bring the connected Car Thing to the latest bridgething release

The starter App connects to ws://127.0.0.1:8891/ on the device, and to
ws://<device-ip>:8891/ in dev when you set VITE_BRIDGETHING_URL.
`);
}

main();
