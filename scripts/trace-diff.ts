#!/usr/bin/env bun
// Diffs the per-implementation trace files a conformance corpus emits.
// Usage: bun scripts/trace-diff.ts <corpus-basename>   (default: pacer-trace)

import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const IMPLS = ['rust', 'swift', 'kotlin'] as const;
const FIXTURES = join(import.meta.dir, '..', 'crates', 'lib', 'fixtures');

type Step = Record<string, unknown> & { t_ms: number };
type Case = { name: string; steps: Step[] };
type Emitted = { impl: string; constants: Record<string, unknown>; cases: Case[] };

const base = process.argv[2] ?? 'pacer-trace';

const emitted = new Map<string, Emitted>();
for (const impl of IMPLS) {
  const path = join(FIXTURES, `${base}.${impl}.json`);
  if (!existsSync(path)) {
    console.error(`missing ${base}.${impl}.json - run that language's emitter first`);
    process.exit(2);
  }
  emitted.set(impl, JSON.parse(readFileSync(path, 'utf8')));
}

const present = [...emitted.keys()];
const norm = (v: unknown) => (v === undefined || v === null ? 'null' : JSON.stringify(v));
const disagree = (vals: unknown[]) => new Set(vals.map(norm)).size > 1;

let divergences = 0;

console.log('=== constants ===');
const constantKeys = [...new Set(present.flatMap(i => Object.keys(emitted.get(i)!.constants)))].sort();
console.log('key'.padEnd(24) + present.map(i => i.padStart(14)).join(''));
for (const key of constantKeys) {
  const vals = present.map(i => emitted.get(i)!.constants[key]);
  const differs = disagree(vals);
  if (differs) divergences++;
  console.log(key.padEnd(24) + vals.map(v => norm(v).padStart(14)).join('') + (differs ? '   <-- differs' : ''));
}

console.log('\n=== steps ===');
const reference = emitted.get(present[0])!;
for (const [caseIndex, refCase] of reference.cases.entries()) {
  for (const impl of present) {
    const other = emitted.get(impl)!.cases[caseIndex];
    if (other?.name !== refCase.name) {
      console.error(`case ${caseIndex} is "${refCase.name}" in ${present[0]} but "${other?.name}" in ${impl}`);
      process.exit(2);
    }
  }

  for (const [stepIndex, refStep] of refCase.steps.entries()) {
    const steps = present.map(i => emitted.get(i)!.cases[caseIndex].steps[stepIndex]);
    const fields = [...new Set(steps.flatMap(s => Object.keys(s)))].filter(f => f !== 't_ms').sort();
    for (const field of fields) {
      const vals = steps.map(s => s[field]);
      if (!disagree(vals)) continue;
      divergences++;
      console.log(`\n${refCase.name}   t=${refStep.t_ms}ms   ${field}`);
      present.forEach((impl, i) => console.log(`    ${impl.padEnd(8)}${norm(vals[i])}`));
    }
  }
}

const cases = reference.cases.length;
console.log(`\n${divergences} divergence(s) across ${cases} case(s), ${present.length} implementations`);
process.exit(0);
