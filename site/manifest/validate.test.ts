import { describe, expect, test } from 'bun:test';
import type { DiscoverManifest } from './types.ts';
import { ManifestValidationError, validate, validateInvariants } from './validate.ts';

function fixture(): DiscoverManifest {
  return {
    manifest_version: 1,
    updated_at: '2026-05-07T00:00:00Z',
    project: {
      id: 'bridgething',
      name: 'bridgething',
      description: 'test',
      publisher: 'JoeyEamigh',
      publisher_url: null,
      license: null,
      website: null,
      source_url: null,
      issue_url: null,
      support_url: null,
      icon_url: null,
      banner_url: null,
      screenshots: [],
    },
    channels: {
      stable: {
        name: 'Stable',
        description: 'Stable',
        stability: 'stable',
        default: true,
        latest: '0.1.0+image.2026.01.0',
        releases: ['0.1.0+image.2026.01.0', '0.0.1+image.2026.01.0'],
      },
    },
    releases: {
      '0.0.1+image.2026.01.0': releaseEntry('0.0.1+image.2026.01.0', 'stable'),
      '0.1.0+image.2026.01.0': releaseEntry('0.1.0+image.2026.01.0', 'stable'),
    },
  };
}

function releaseEntry(version: string, channel: string) {
  return {
    version,
    channel,
    released_at: '2026-01-01T12:00:00Z',
    summary: 'test',
    changelog: '## body',
    changelog_url: null,
    yanked: null,
    deprecated: false,
    download: {
      url: 'https://example.com/x.zip',
      size: 1,
      sha256: '0000000000000000000000000000000000000000000000000000000000000000',
    },
  };
}

describe('validateInvariants()', () => {
  test('happy path passes', () => {
    expect(() => validateInvariants(fixture())).not.toThrow();
  });

  test('fails when release version disagrees with map key', () => {
    const m = fixture();
    m.releases['0.0.1+image.2026.01.0']!.version = '0.0.2+image.2026.01.0';
    expect(() => validateInvariants(m)).toThrow(ManifestValidationError);
  });

  test('fails when release.channel references missing channel', () => {
    const m = fixture();
    m.releases['0.0.1+image.2026.01.0']!.channel = 'ghost';
    expect(() => validateInvariants(m)).toThrow(/not present in channels/);
  });

  test('fails when channel.latest is unknown', () => {
    const m = fixture();
    m.channels['stable']!.latest = '9.9.9+image.9999.99.9';
    expect(() => validateInvariants(m)).toThrow(/latest=.* not present in releases/);
  });

  test('fails when channel.latest is withdrawn but the channel still has an installable release', () => {
    const yanked = fixture();
    yanked.releases['0.1.0+image.2026.01.0']!.yanked = 'bricks wifi';
    expect(() => validateInvariants(yanked)).toThrow(/is withdrawn while the channel still has an installable/);

    const deprecated = fixture();
    deprecated.releases['0.1.0+image.2026.01.0']!.deprecated = true;
    expect(() => validateInvariants(deprecated)).toThrow(/is withdrawn while the channel still has an installable/);
  });

  test('allows a withdrawn latest when every release on the channel is withdrawn', () => {
    const m = fixture();
    for (const release of Object.values(m.releases)) release.yanked = 'bricks wifi';
    expect(() => validateInvariants(m)).not.toThrow();
  });

  test('fails when channel.releases lists a release whose channel disagrees', () => {
    const m = fixture();
    m.channels['dev'] = {
      name: 'Dev',
      description: 'Dev',
      stability: 'experimental',
      default: false,
      latest: '0.0.1+image.2026.01.0',
      releases: ['0.0.1+image.2026.01.0'],
    };
    expect(() => validateInvariants(m)).toThrow(/listed in both/);
  });

  test('fails on orphaned release', () => {
    const m = fixture();
    m.releases['0.2.0+image.2026.02.0'] = releaseEntry('0.2.0+image.2026.02.0', 'stable');
    expect(() => validateInvariants(m)).toThrow(/orphaned/);
  });

  test('fails when more than one channel marks default', () => {
    const m = fixture();
    m.channels['dev'] = {
      name: 'Dev',
      description: 'Dev',
      stability: 'experimental',
      default: true,
      latest: '0.0.1+image.2026.01.0',
      releases: [],
    };
    expect(() => validateInvariants(m)).toThrow(/at most one channel/);
  });

  test('validates a release with artifacts present', () => {
    const m = fixture();
    const sha256 = '0'.repeat(64);
    m.releases['0.1.0+image.2026.01.0']!.artifacts = {
      daemon: { size: 123, sha256 },
      image_swu: { size: 456, sha256 },
      image_zck: { size: 789, sha256 },
      image_boot_zck: { size: 101, sha256 },
      webapps: { hub: { size: 1, sha256 }, stock: { size: 2, sha256 } },
    };
    expect(() => validate(m)).not.toThrow();
  });

  test('validates a release with artifacts absent', () => {
    const m = fixture();
    expect('artifacts' in m.releases['0.1.0+image.2026.01.0']!).toBe(false);
    expect(() => validate(m)).not.toThrow();
  });

  test('rejects an artifact digest with a malformed sha256', () => {
    const m = fixture();
    m.releases['0.1.0+image.2026.01.0']!.artifacts = {
      daemon: { size: 123, sha256: 'not-hex' },
    };
    expect(() => validate(m)).toThrow(ManifestValidationError);
  });
});
