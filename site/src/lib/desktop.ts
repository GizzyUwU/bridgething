import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';

export type DesktopPointer = {
  version: string;
  released_at: string;
  platforms: Record<string, DesktopBuild>;
};

export type DesktopBuild = {
  url: string;
  size: number;
  sha256: string;
};

export type DesktopRow = {
  target: string;
  os: string;
  arch: string;
  artifact: string;
  build: DesktopBuild | null;
};

const FIXTURE_PATH = resolve(process.cwd(), 'manifest', 'desktop.json');

const TARGETS = [
  'darwin-aarch64',
  'darwin-x86_64',
  'linux-x86_64',
  'linux-aarch64',
  'windows-x86_64',
  'windows-aarch64',
];

let cached: DesktopPointer | null = null;

export async function loadDesktop(): Promise<DesktopPointer | null> {
  if (cached) return cached;
  let raw: string;
  try {
    raw = await readFile(FIXTURE_PATH, 'utf-8');
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === 'ENOENT') return null;
    throw err;
  }
  cached = parse(JSON.parse(raw) as unknown);
  return cached;
}

function parse(input: unknown): DesktopPointer {
  if (typeof input !== 'object' || input === null) throw new Error('desktop.json must be an object');
  const root = input as Record<string, unknown>;
  for (const key of ['version', 'released_at']) {
    if (typeof root[key] !== 'string') throw new Error(`desktop.json ${key} must be a string`);
  }

  const platforms = root['platforms'];
  if (typeof platforms !== 'object' || platforms === null) throw new Error('desktop.json is missing platforms');
  for (const [target, entry] of Object.entries(platforms as Record<string, unknown>)) {
    if (typeof entry !== 'object' || entry === null) {
      throw new Error(`desktop.json platforms.${target} must be an object`);
    }
    const build = entry as Record<string, unknown>;
    for (const key of ['url', 'sha256']) {
      if (typeof build[key] !== 'string') throw new Error(`desktop.json platforms.${target}.${key} must be a string`);
    }
    if (typeof build['size'] !== 'number') throw new Error(`desktop.json platforms.${target}.size must be a number`);
  }

  return root as unknown as DesktopPointer;
}

export function desktopRows(pointer: DesktopPointer | null): DesktopRow[] {
  const published = pointer ? Object.keys(pointer.platforms) : [];
  const targets = [...TARGETS, ...published.filter(target => !TARGETS.includes(target))];

  return targets.map(target => {
    const [os = target, arch = ''] = target.split('-');
    const build = pointer?.platforms[target] ?? null;
    return {
      target,
      os: osLabel(os),
      arch: archLabel(os, arch),
      artifact: build ? artifactLabel(build.url) : expectedArtifact(os),
      build,
    };
  });
}

function osLabel(os: string): string {
  return os === 'darwin' ? 'macos' : os;
}

function archLabel(os: string, arch: string): string {
  if (os === 'darwin') return arch === 'aarch64' ? 'apple silicon' : 'intel';
  return arch === 'aarch64' ? 'arm64' : arch;
}

function expectedArtifact(os: string): string {
  if (os === 'darwin') return 'app bundle (.app.tar.gz)';
  if (os === 'linux') return 'appimage';
  if (os === 'windows') return 'msi or exe installer';
  return 'archive';
}

export function artifactLabel(url: string): string {
  const name = url.split('/').pop() ?? url;
  if (name.endsWith('.app.tar.gz')) return 'app bundle (.app.tar.gz)';
  if (name.endsWith('.dmg')) return 'dmg';
  if (name.endsWith('.AppImage')) return 'appimage';
  if (name.endsWith('.msi')) return 'msi installer';
  if (name.endsWith('.exe')) return 'exe installer';
  if (name.endsWith('.tar.gz')) return 'tar.gz archive';
  if (name.endsWith('.zip')) return 'zip archive';
  return name;
}
