import { describe, expect, test } from 'bun:test';
import type { Catalog } from '../src/types.ts';
import { CatalogValidationError, validate, validateInvariants, validateSchema } from '../src/validate.ts';

const CALENDAR_ID = '019e6701-13f8-71b5-ba04-85d326630e98';
const WEATHER_ID = '019e6701-13f8-71b5-ba04-81f347137de2';
const SHA = '0'.repeat(64);

function version(v: string, releasedAt: string) {
  return {
    version: v,
    released_at: releasedAt,
    download: { url: 'https://apps.bridgething.com/r/x.zip', size: 1, sha256: SHA },
    permissions: ['net.fetch'],
    min_libbridgething_version: '0.5.0',
    changelog: null,
  };
}

function fixture(): Catalog {
  return {
    schema: 'catalog.v1',
    updated_at: '2026-05-31T00:00:00Z',
    repo: { name: 'bridgething apps', description: 'official', homepage: null, icon: null },
    apps: [
      {
        id: CALENDAR_ID,
        name: 'Calendar',
        description: 'Upcoming events.',
        author: 'JoeyEamigh',
        icon: null,
        homepage: null,
        source: null,
        versions: [version('0.2.0', '2026-05-31T00:00:00Z'), version('0.1.0', '2026-05-01T00:00:00Z')],
      },
    ],
    recommended_sources: [],
  };
}

describe('validateSchema()', () => {
  test('happy path passes', () => {
    expect(() => validateSchema(fixture())).not.toThrow();
  });

  test('rejects unknown top-level key', () => {
    const m = fixture() as Record<string, unknown>;
    m['extra'] = true;
    expect(() => validateSchema(m)).toThrow(CatalogValidationError);
  });

  test('rejects a non-catalog schema discriminant', () => {
    const m = fixture() as unknown as { schema: string };
    m.schema = 'catalog.v2';
    expect(() => validateSchema(m)).toThrow(/schema validation/);
  });

  test('rejects a malformed sha256', () => {
    const m = fixture();
    m.apps[0]!.versions[0]!.download.sha256 = 'nope';
    expect(() => validateSchema(m)).toThrow(CatalogValidationError);
  });

  test('rejects an app with no versions', () => {
    const m = fixture();
    m.apps[0]!.versions = [];
    expect(() => validateSchema(m)).toThrow(CatalogValidationError);
  });
});

describe('validateInvariants()', () => {
  test('happy path passes', () => {
    expect(() => validateInvariants(fixture())).not.toThrow();
  });

  test('fails on duplicate app ids', () => {
    const m = fixture();
    m.apps.push({ ...m.apps[0]!, name: 'Calendar Clone' });
    expect(() => validateInvariants(m)).toThrow(/used by both/);
  });

  test('fails when an app id is not a uuidv7', () => {
    const m = fixture();
    m.apps[0]!.id = '00000000-0000-4000-8000-000000000000';
    expect(() => validateInvariants(m)).toThrow(/not a valid uuidv7/);
  });

  test('fails on a duplicate version within one app', () => {
    const m = fixture();
    m.apps[0]!.versions.push(version('0.1.0', '2026-04-01T00:00:00Z'));
    expect(() => validateInvariants(m)).toThrow(/more than once/);
  });

  test('fails when versions are not newest-first', () => {
    const m = fixture();
    m.apps[0]!.versions = [version('0.1.0', '2026-05-01T00:00:00Z'), version('0.2.0', '2026-05-31T00:00:00Z')];
    expect(() => validateInvariants(m)).toThrow(/not newest-first/);
  });
});

describe('validate()', () => {
  test('passes a multi-app catalog', () => {
    const m = fixture();
    m.apps.push({
      id: WEATHER_ID,
      name: 'Weather',
      description: 'Conditions and forecast.',
      author: 'JoeyEamigh',
      icon: null,
      homepage: null,
      source: null,
      versions: [version('0.1.0', '2026-05-31T00:00:00Z')],
    });
    expect(() => validate(m)).not.toThrow();
  });
});
