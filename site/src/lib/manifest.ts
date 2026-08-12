import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import type { DiscoverManifest } from '../../manifest/types.ts';
import { validate } from '../../manifest/validate.ts';

const FIXTURE_PATH = resolve(process.cwd(), 'manifest', 'fixture.json');

let cached: DiscoverManifest | null = null;

export async function loadManifest(): Promise<DiscoverManifest | null> {
  if (cached) return cached;
  let raw: string;
  try {
    raw = await readFile(FIXTURE_PATH, 'utf-8');
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === 'ENOENT') return null;
    throw err;
  }
  const parsed = JSON.parse(raw) as unknown;
  cached = validate(parsed);
  return cached;
}
