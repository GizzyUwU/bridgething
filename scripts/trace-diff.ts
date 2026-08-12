#!/usr/bin/env bun
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const FIXTURES = join(import.meta.dir, '..', 'crates', 'lib', 'fixtures');

type Step = Record<string, unknown>;
type Case = Record<string, unknown> & { name?: string; id?: string };
type Emitted = {
  impl: string;
  constants: Record<string, unknown>;
  unobserved?: string[];
  cases: Case[];
};

const base = process.argv[2] ?? 'pacer-trace';

const corpusPath = join(FIXTURES, `${base}.json`);
if (!existsSync(corpusPath)) {
  console.error(`missing ${base}.json - that is the input corpus, not an emitted answer file`);
  process.exit(2);
}
const corpus = JSON.parse(readFileSync(corpusPath, 'utf8')) as {
  arms?: string[] | Record<string, string>;
  cases?: { name?: string; id?: string; arms?: string[] }[];
};
const arms = Array.isArray(corpus.arms)
  ? corpus.arms
  : corpus.arms
    ? Object.keys(corpus.arms)
    : ['rust', 'swift', 'kotlin'];
const declaredArms = new Map((corpus.cases ?? []).map(c => [c.name ?? c.id, c.arms]));

const emitted = new Map<string, Emitted>();
for (const impl of arms) {
  const path = join(FIXTURES, `${base}.${impl}.json`);
  if (!existsSync(path)) {
    console.error(`missing ${base}.${impl}.json - run that arm's emitter first`);
    process.exit(2);
  }
  emitted.set(impl, JSON.parse(readFileSync(path, 'utf8')));
}

const present = [...emitted.keys()];
const width = Math.max(14, ...present.map(i => i.length + 2));
const label = Math.max(8, ...present.map(i => i.length + 2));
const norm = (v: unknown) => (v === undefined || v === null ? 'null' : JSON.stringify(v));
const disagree = (vals: unknown[]) => new Set(vals.map(norm)).size > 1;
const observes = (impl: string, field: string) => !(emitted.get(impl)!.unobserved ?? []).includes(field);
const caseName = (c: Case | undefined) => (c?.name ?? c?.id) as string | undefined;
const sequence = (c: Case | undefined) => (c?.steps ?? c?.events ?? []) as Step[];

let divergences = 0;

console.log('=== constants ===');
const constantKeys = [...new Set(present.flatMap(i => Object.keys(emitted.get(i)!.constants)))].sort();
console.log('key'.padEnd(24) + present.map(i => i.padStart(width)).join(''));
for (const key of constantKeys) {
  const vals = present.map(i => emitted.get(i)!.constants[key]);
  const differs = disagree(vals);
  if (differs) divergences++;
  console.log(key.padEnd(24) + vals.map(v => norm(v).padStart(width)).join('') + (differs ? '   <-- differs' : ''));
}

function compareField(where: string, field: string, vals: Map<string, unknown>): boolean {
  const observers = present.filter(i => vals.has(i));
  if (observers.length < 2) return false;
  const values = observers.map(i => vals.get(i));
  if (!disagree(values)) return false;
  divergences++;
  console.log(`\n${where}   ${field}`);
  observers.forEach((impl, i) => console.log(`    ${impl.padEnd(label)}${norm(values[i])}`));
  return true;
}

console.log('\n=== steps ===');
const reference = emitted.get(present[0])!;
for (const [caseIndex, refCase] of reference.cases.entries()) {
  const name = caseName(refCase) ?? String(caseIndex);
  const declared = declaredArms.get(name);
  const participants = present.filter(i => !declared || declared.includes(i));
  for (const impl of participants) {
    const other = emitted.get(impl)!.cases[caseIndex];
    if (caseName(other) !== name) {
      console.error(`case ${caseIndex} is "${name}" in ${present[0]} but "${caseName(other)}" in ${impl}`);
      process.exit(2);
    }
  }

  const sequences = new Map(participants.map(i => [i, sequence(emitted.get(i)!.cases[caseIndex])]));
  const lengths = participants.map(i => sequences.get(i)!.length);
  const shortest = Math.min(...lengths);
  if (new Set(lengths).size > 1) {
    divergences++;
    console.log(`\n${name}   sequence length`);
    participants.forEach((impl, i) => console.log(`    ${impl.padEnd(label)}${lengths[i]}`));
  }

  for (let stepIndex = 0; stepIndex < shortest; stepIndex++) {
    const steps = new Map(participants.map(i => [i, sequences.get(i)![stepIndex]]));
    const marker = steps.get(participants[0])!;
    const tag = marker.t_ms !== undefined ? `t=${marker.t_ms}ms` : `step ${stepIndex}`;
    const fields = [...new Set(participants.flatMap(i => Object.keys(steps.get(i)!)))].filter(f => f !== 't_ms').sort();
    for (const field of fields) {
      const vals = new Map(participants.filter(i => observes(i, field)).map(i => [i, steps.get(i)![field]]));
      compareField(`${name}   ${tag}`, field, vals);
    }
  }

  if (new Set(lengths).size > 1) {
    console.log(`\n${name}   steps past the common prefix`);
    for (const impl of participants) {
      for (const [offset, step] of sequences.get(impl)!.slice(shortest).entries()) {
        console.log(`    ${impl.padEnd(label)}[${shortest + offset}] ${JSON.stringify(step)}`);
      }
    }
  }

  const summaryKeys = Object.keys(refCase).filter(k => {
    const v = refCase[k];
    return v !== null && typeof v === 'object' && !Array.isArray(v);
  });
  for (const key of summaryKeys) {
    const objects = new Map(
      participants.map(i => [i, (emitted.get(i)!.cases[caseIndex][key] ?? {}) as Record<string, unknown>]),
    );
    const inner = [...new Set(participants.flatMap(i => Object.keys(objects.get(i)!)))].sort();
    for (const field of inner) {
      const path = `${key}.${field}`;
      const vals = new Map(participants.filter(i => observes(i, path)).map(i => [i, objects.get(i)![field]]));
      compareField(`${name}   ${key}`, field, vals);
    }
  }
}

const cases = reference.cases.length;
console.log(`\n${divergences} divergence(s) across ${cases} case(s), ${present.length} implementations`);
process.exit(0);
