#!/usr/bin/env bun
import { writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { readSurfaceDocs } from './sources.ts';

const here = new URL('.', import.meta.url).pathname;
const repoRoot = resolve(here, '..', '..');
const bridgethingRepo = process.env['BRIDGETHING_REPO'] ?? repoRoot;
const outPath = resolve(here, 'surfaces.json');

const docs = await readSurfaceDocs(bridgethingRepo);
if (docs === null) {
  console.log(`no surfaces.json under ${bridgethingRepo}; leaving sdk/surfaces.json as-is`);
  process.exit(0);
}

await writeFile(outPath, `${JSON.stringify(docs, null, 2)}\n`);
console.log(
  `wrote ${outPath} (${docs.surfaces.length} surfaces, ${Object.keys(docs.types).length} types, v${docs.version})`,
);
