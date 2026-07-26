import { describe, expect, test } from 'bun:test';
import { readFile } from 'node:fs/promises';
import { buildValidatorSources, VALIDATOR_PATH, VALIDATOR_TYPES_PATH } from '../scripts/compile-validator.ts';

describe('the committed validator', () => {
  test('matches what schema.v1.json compiles to', async () => {
    const [built, committed, committedTypes] = await Promise.all([
      buildValidatorSources(),
      readFile(VALIDATOR_PATH, 'utf-8'),
      readFile(VALIDATOR_TYPES_PATH, 'utf-8'),
    ]);

    expect(built.code).toBe(committed);
    expect(built.types).toBe(committedTypes);
  });

  test('imports nothing, since the workers runtime cannot resolve ajv helpers', async () => {
    const committed = await readFile(VALIDATOR_PATH, 'utf-8');
    expect(committed).not.toMatch(/^\s*import\s.+\sfrom\s/m);
  });
});
