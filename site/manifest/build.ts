#!/usr/bin/env bun
import { writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { generate, loadBundles, loadProjectAndChannels, stringify } from './generate.ts';
import { readComponentReleases } from './sources.ts';

const here = new URL('.', import.meta.url).pathname;
const repoRoot = resolve(here, '..', '..');

const bridgethingRepo = process.env['BRIDGETHING_REPO'] ?? repoRoot;
const yoctoRepo = process.env['YOCTO_SUPERBIRD_REPO'] ?? resolve(repoRoot, '..', 'yocto-superbird');

const configPath = resolve(here, 'project.yaml');
const bundlesPath = resolve(here, 'bundles.yaml');
const outPath = resolve(here, 'fixture.json');

const [{ project, channels }, daemonReleases, imageReleases, bundles] = await Promise.all([
  loadProjectAndChannels(configPath),
  readComponentReleases(resolve(bridgethingRepo, 'releases')),
  readComponentReleases(resolve(yoctoRepo, 'releases')),
  loadBundles(bundlesPath),
]);

if (bundles.length === 0) {
  console.log('no bundles configured; skipping fixture write (loadManifest() will return null)');
  process.exit(0);
}

const missing: string[] = [];
if (daemonReleases.length === 0) missing.push(resolve(bridgethingRepo, 'releases'));
if (imageReleases.length === 0) missing.push(resolve(yoctoRepo, 'releases'));

if (missing.length > 0) {
  console.log(`no release notes under ${missing.join(' or ')}; keeping the existing ${outPath}`);
  process.exit(0);
}

const manifest = generate({
  project,
  channels,
  daemonReleases,
  imageReleases,
  bundles,
  updatedAt: new Date().toISOString().replace(/\.\d{3}Z$/, 'Z'),
});

await writeFile(outPath, stringify(manifest));
console.log(
  `wrote ${outPath} (${Object.keys(manifest.releases).length} releases across ${Object.keys(manifest.channels).length} channels)`,
);
