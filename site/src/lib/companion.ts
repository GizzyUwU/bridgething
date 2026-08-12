import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';

export type CompanionPointer = {
  android: AndroidBuild;
};

export type AndroidBuild = {
  version: string;
  url: string;
  size: number;
  sha256: string;
  released_at: string;
};

const FIXTURE_PATH = resolve(process.cwd(), 'manifest', 'companion.json');

let cached: CompanionPointer | null = null;

export async function loadCompanion(): Promise<CompanionPointer | null> {
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

function parse(input: unknown): CompanionPointer {
  if (typeof input !== 'object' || input === null) throw new Error('companion.json must be an object');
  const android = (input as Record<string, unknown>)['android'];
  if (typeof android !== 'object' || android === null) throw new Error('companion.json is missing android');

  const build = android as Record<string, unknown>;
  for (const key of ['version', 'url', 'sha256', 'released_at']) {
    if (typeof build[key] !== 'string') throw new Error(`companion.json android.${key} must be a string`);
  }
  if (typeof build['size'] !== 'number') throw new Error('companion.json android.size must be a number');

  return { android: build as unknown as AndroidBuild };
}

export function formatSize(bytes: number): string {
  const mb = bytes / 1024 / 1024;
  return mb >= 100 ? `${Math.round(mb)} mb` : `${mb.toFixed(1)} mb`;
}
