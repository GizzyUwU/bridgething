#!/usr/bin/env bun
import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { parse, stringify } from 'yaml';
import { validate, type RecommendedSource } from '@bridgething/catalog';

const DEFAULT_DIRECTORY_URL = 'https://bridgething.com/api/sources.json';

function parseArgs(argv: string[]): { directoryUrl: string; statePath: string } {
  const out: Record<string, string> = {};
  for (let i = 0; i < argv.length; i++) {
    const next = argv[i + 1];
    if (!next) continue;
    if (argv[i] === '--directory-url') out.directoryUrl = next;
    if (argv[i] === '--state-path') out.statePath = next;
  }
  return {
    directoryUrl: out.directoryUrl ?? DEFAULT_DIRECTORY_URL,
    statePath: out.statePath ?? resolve(import.meta.dirname, '..', 'apps', 'apps-published.yaml'),
  };
}

const args = parseArgs(process.argv.slice(2));

const response = await fetch(args.directoryUrl, { redirect: 'follow' });
if (!response.ok) {
  throw new Error(`${args.directoryUrl} returned ${response.status} ${response.statusText}`);
}

const directory = validate(await response.json());

const recommended: RecommendedSource[] = directory.recommended_sources
  .filter(source => source.attested)
  .map(source => ({
    name: source.name,
    url: source.url,
    description: source.description,
    attested: true,
  }));

const raw = await readFile(args.statePath, 'utf-8');
const doc = parse(raw) as Record<string, unknown>;
doc['recommended_sources'] = recommended;

await writeFile(args.statePath, stringify(doc, { indent: 2 }));

const listed = directory.recommended_sources.length - recommended.length;
console.log(
  `synced ${recommended.length} attested source(s) into ${args.statePath} ` +
    `(${listed} listed source(s) left to the directory)`,
);
