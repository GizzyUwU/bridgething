#!/usr/bin/env bun
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

const SOURCE = resolve(import.meta.dir, '..', '..', 'webapp-shared', 'src', 'push.ts');
const DEST = resolve(import.meta.dir, '..', 'template', 'scripts', 'push.ts');

const HEADER = `#!/usr/bin/env bun
// Build this webapp and install it onto a connected Car Thing, then make it
// visible: a plain app becomes active, a launcher takes the home-screen slot,
// an overlay takes the overlay slot. Run with --help for the flags.
//
// Generated from @bridgething/webapp-shared. It is yours now; tweak it freely.
`;

const TAIL = `
bridgethingPush({ scriptUrl: import.meta.url }).catch(err => {
  console.error(err instanceof Error ? err.message : err);
  process.exit(1);
});
`;

writeFileSync(DEST, `${HEADER}${readFileSync(SOURCE, 'utf8')}${TAIL}`);
console.log(`wrote ${DEST}`);
