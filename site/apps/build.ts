#!/usr/bin/env bun
import { writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { generate, loadCuration, loadPublishedState, mergeApps, stringify } from './generate.ts';

const here = new URL('.', import.meta.url).pathname;

const curationPath = resolve(here, 'apps.yaml');
const statePath = resolve(here, 'apps-published.yaml');
const outPath = resolve(here, 'catalog.json');

const [curation, state] = await Promise.all([loadCuration(curationPath), loadPublishedState(statePath)]);

const catalog = generate({
  repo: curation.repo,
  recommendedSources: state.recommended_sources ?? [],
  apps: mergeApps(curation, state),
  updatedAt: new Date().toISOString().replace(/\.\d{3}Z$/, 'Z'),
});

await writeFile(outPath, stringify(catalog));
console.log(
  `wrote ${outPath} (${catalog.apps.length} apps, ${catalog.apps.reduce((n, a) => n + a.versions.length, 0)} versions)`,
);
