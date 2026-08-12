import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import type { SurfaceDocs } from './types.ts';

export function docsSourcePath(bridgethingRepo: string): string {
  return resolve(bridgethingRepo, 'crates', 'lib', 'docs', 'surfaces.json');
}

export async function readSurfaceDocs(bridgethingRepo: string): Promise<SurfaceDocs | null> {
  let raw: string;
  try {
    raw = await readFile(docsSourcePath(bridgethingRepo), 'utf-8');
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === 'ENOENT') return null;
    throw err;
  }
  return validate(JSON.parse(raw) as unknown);
}

export function validate(value: unknown): SurfaceDocs {
  if (typeof value !== 'object' || value === null) throw new Error('surface docs: not an object');
  const v = value as Record<string, unknown>;
  if (typeof v['version'] !== 'string') throw new Error('surface docs: missing string "version"');
  if (!Array.isArray(v['surfaces'])) throw new Error('surface docs: "surfaces" is not an array');
  if (typeof v['types'] !== 'object' || v['types'] === null) throw new Error('surface docs: "types" is not an object');
  for (const s of v['surfaces'] as unknown[]) {
    const surface = s as Record<string, unknown>;
    if (typeof surface['name'] !== 'string' || typeof surface['title'] !== 'string') {
      throw new Error('surface docs: a surface is missing "name"/"title"');
    }
    for (const group of ['events', 'requests', 'commands', 'handlers'] as const) {
      if (!Array.isArray(surface[group]))
        throw new Error(`surface docs: ${String(surface['name'])}.${group} is not an array`);
    }
  }
  return value as SurfaceDocs;
}
