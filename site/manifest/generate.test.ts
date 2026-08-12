import { describe, expect, test } from 'bun:test';
import { mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { generate, type BundleEntry } from './generate.ts';
import { readComponentReleases } from './sources.ts';
import type { ChannelSource, ProjectSource } from './types.ts';

const project: ProjectSource = {
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
};

const channels: ChannelSource[] = [
  {
    slug: 'stable',
    name: 'Stable',
    description: 'Stable',
    stability: 'stable',
    default: true,
  },
  {
    slug: 'dev',
    name: 'Dev',
    description: 'Dev',
    stability: 'experimental',
    default: false,
  },
];

const sha = '0'.repeat(64);

function bundle(
  daemon: string,
  image: string,
  channel: string,
  when: string,
  opts: Partial<BundleEntry> = {},
): BundleEntry {
  return {
    daemonVersion: daemon,
    imageVersion: image,
    channel,
    releasedAt: when,
    daemonBumped: opts.daemonBumped ?? true,
    imageBumped: opts.imageBumped ?? false,
    download: {
      url: `https://ota.bridgething.com/r/x.zip`,
      size: 1,
      sha256: sha,
    },
    ...opts,
  };
}

async function writeRelease(dir: string, version: string, frontmatter: Record<string, unknown>, body: string) {
  await mkdir(dir, { recursive: true });
  const fm = Object.entries(frontmatter)
    .map(([k, v]) => `${k}: ${v === null ? 'null' : v}`)
    .join('\n');
  await writeFile(join(dir, `${version}.md`), `---\n${fm}\n---\n\n${body}\n`);
}

describe('generate()', () => {
  test('composes a manifest from one daemon-bump bundle', async () => {
    const tmp = await mkdtemp(join(tmpdir(), 'btmf-'));
    const daemonDir = join(tmp, 'daemon');
    const imageDir = join(tmp, 'image');

    await writeRelease(
      daemonDir,
      '0.1.0',
      {
        version: '0.1.0',
        channel: 'stable',
        released_at: '2026-02-15T18:00:00Z',
        summary: 'ANCS notifications.',
      },
      '## Highlights\n\n- ANCS.\n',
    );

    await writeRelease(
      imageDir,
      '2026.01.0',
      {
        version: '2026.01.0',
        channel: 'stable',
        released_at: '2026-01-01T12:00:00Z',
        summary: 'Initial image.',
      },
      '## Highlights\n\n- Kernel.\n',
    );

    const [daemonReleases, imageReleases] = await Promise.all([
      readComponentReleases(daemonDir),
      readComponentReleases(imageDir),
    ]);

    const manifest = generate({
      project,
      channels,
      daemonReleases,
      imageReleases,
      bundles: [bundle('0.1.0', '2026.01.0', 'stable', '2026-02-15T18:00:00Z')],
      updatedAt: '2026-02-15T18:00:01Z',
    });

    expect(manifest.releases['0.1.0+image.2026.01.0']).toBeTruthy();
    expect(manifest.channels['stable']!.latest).toBe('0.1.0+image.2026.01.0');
    expect(manifest.releases['0.1.0+image.2026.01.0']!.summary).toBe('ANCS notifications.');
    expect(manifest.releases['0.1.0+image.2026.01.0']!.changelog).toContain('ANCS');
    expect(manifest.releases['0.1.0+image.2026.01.0']!.changelog).toContain('_no change since previous release._');
  });

  test('emits builtin_webapps when the bundle carries them, omits the key otherwise', async () => {
    const tmp = await mkdtemp(join(tmpdir(), 'btmf-'));
    const daemonDir = join(tmp, 'daemon');
    const imageDir = join(tmp, 'image');
    await writeRelease(
      daemonDir,
      '0.1.0',
      { version: '0.1.0', channel: 'stable', released_at: '2026-02-15T18:00:00Z', summary: 'd.' },
      'body',
    );
    await writeRelease(
      imageDir,
      '2026.01.0',
      { version: '2026.01.0', channel: 'stable', released_at: '2026-01-01T12:00:00Z', summary: 'i.' },
      'body',
    );
    const [daemonReleases, imageReleases] = await Promise.all([
      readComponentReleases(daemonDir),
      readComponentReleases(imageDir),
    ]);

    const withApps = generate({
      project,
      channels,
      daemonReleases,
      imageReleases,
      bundles: [
        bundle('0.1.0', '2026.01.0', 'stable', '2026-02-15T18:00:00Z', {
          builtinWebapps: { hub: '0.1.0', stock: '8.9.2' },
        }),
      ],
      updatedAt: '2026-02-15T18:00:01Z',
    });
    expect(withApps.releases['0.1.0+image.2026.01.0']!.builtin_webapps).toEqual({ hub: '0.1.0', stock: '8.9.2' });

    const without = generate({
      project,
      channels,
      daemonReleases,
      imageReleases,
      bundles: [bundle('0.1.0', '2026.01.0', 'stable', '2026-02-15T18:00:00Z')],
      updatedAt: '2026-02-15T18:00:01Z',
    });
    expect('builtin_webapps' in without.releases['0.1.0+image.2026.01.0']!).toBe(false);
  });

  test('emits artifacts when the bundle carries them, omits the key otherwise', async () => {
    const tmp = await mkdtemp(join(tmpdir(), 'btmf-'));
    const daemonDir = join(tmp, 'daemon');
    const imageDir = join(tmp, 'image');
    await writeRelease(
      daemonDir,
      '0.1.0',
      { version: '0.1.0', channel: 'stable', released_at: '2026-02-15T18:00:00Z', summary: 'd.' },
      'body',
    );
    await writeRelease(
      imageDir,
      '2026.01.0',
      { version: '2026.01.0', channel: 'stable', released_at: '2026-01-01T12:00:00Z', summary: 'i.' },
      'body',
    );
    const [daemonReleases, imageReleases] = await Promise.all([
      readComponentReleases(daemonDir),
      readComponentReleases(imageDir),
    ]);

    const artifacts = {
      daemon: { size: 123, sha256: sha },
      image_swu: { size: 456, sha256: sha },
      image_zck: { size: 789, sha256: sha },
      image_boot_zck: { size: 101, sha256: sha },
      webapps: { hub: { size: 1, sha256: sha }, stock: { size: 2, sha256: sha } },
    };

    const withArtifacts = generate({
      project,
      channels,
      daemonReleases,
      imageReleases,
      bundles: [bundle('0.1.0', '2026.01.0', 'stable', '2026-02-15T18:00:00Z', { artifacts })],
      updatedAt: '2026-02-15T18:00:01Z',
    });
    expect(withArtifacts.releases['0.1.0+image.2026.01.0']!.artifacts).toEqual(artifacts);

    const without = generate({
      project,
      channels,
      daemonReleases,
      imageReleases,
      bundles: [bundle('0.1.0', '2026.01.0', 'stable', '2026-02-15T18:00:00Z')],
      updatedAt: '2026-02-15T18:00:01Z',
    });
    expect('artifacts' in without.releases['0.1.0+image.2026.01.0']!).toBe(false);
  });

  test('emits wakeword when the bundle carries it, omits the key otherwise', async () => {
    const tmp = await mkdtemp(join(tmpdir(), 'btmf-'));
    const daemonDir = join(tmp, 'daemon');
    const imageDir = join(tmp, 'image');
    await writeRelease(
      daemonDir,
      '0.1.0',
      { version: '0.1.0', channel: 'stable', released_at: '2026-02-15T18:00:00Z', summary: 'd.' },
      'body',
    );
    await writeRelease(
      imageDir,
      '2026.01.0',
      { version: '2026.01.0', channel: 'stable', released_at: '2026-01-01T12:00:00Z', summary: 'i.' },
      'body',
    );
    const [daemonReleases, imageReleases] = await Promise.all([
      readComponentReleases(daemonDir),
      readComponentReleases(imageDir),
    ]);

    const wakeword = {
      runtime: '0.7.0',
      model: '1.2.0',
      model_trained_against: { '1.2.0': '0.7.0', '1.1.0': '0.6.0' },
    };

    const withWakeWord = generate({
      project,
      channels,
      daemonReleases,
      imageReleases,
      bundles: [bundle('0.1.0', '2026.01.0', 'stable', '2026-02-15T18:00:00Z', { wakeword })],
      updatedAt: '2026-02-15T18:00:01Z',
    });
    expect(withWakeWord.releases['0.1.0+image.2026.01.0']!.wakeword).toEqual(wakeword);

    const without = generate({
      project,
      channels,
      daemonReleases,
      imageReleases,
      bundles: [bundle('0.1.0', '2026.01.0', 'stable', '2026-02-15T18:00:00Z')],
      updatedAt: '2026-02-15T18:00:01Z',
    });
    expect('wakeword' in without.releases['0.1.0+image.2026.01.0']!).toBe(false);
  });

  test('orders channel.releases newest-first', async () => {
    const tmp = await mkdtemp(join(tmpdir(), 'btmf-'));
    const daemonDir = join(tmp, 'daemon');
    const imageDir = join(tmp, 'image');

    await writeRelease(
      daemonDir,
      '0.1.0',
      {
        version: '0.1.0',
        channel: 'stable',
        released_at: '2026-02-15T18:00:00Z',
        summary: 'Second daemon.',
      },
      '## d2',
    );
    await writeRelease(
      daemonDir,
      '0.0.1',
      {
        version: '0.0.1',
        channel: 'stable',
        released_at: '2026-01-01T12:00:00Z',
        summary: 'First daemon.',
      },
      '## d1',
    );
    await writeRelease(
      imageDir,
      '2026.01.0',
      {
        version: '2026.01.0',
        channel: 'stable',
        released_at: '2026-01-01T12:00:00Z',
        summary: 'Initial image.',
      },
      '## i1',
    );

    const [daemonReleases, imageReleases] = await Promise.all([
      readComponentReleases(daemonDir),
      readComponentReleases(imageDir),
    ]);

    const manifest = generate({
      project,
      channels,
      daemonReleases,
      imageReleases,
      bundles: [
        bundle('0.0.1', '2026.01.0', 'stable', '2026-01-01T12:00:00Z', {
          imageBumped: true,
        }),
        bundle('0.1.0', '2026.01.0', 'stable', '2026-02-15T18:00:00Z'),
      ],
      updatedAt: '2026-02-15T18:00:01Z',
    });

    expect(manifest.channels['stable']!.releases).toEqual(['0.1.0+image.2026.01.0', '0.0.1+image.2026.01.0']);
    expect(manifest.channels['stable']!.latest).toBe('0.1.0+image.2026.01.0');
  });

  test('latest falls back past a withdrawn newest release instead of stalling the channel', async () => {
    const tmp = await mkdtemp(join(tmpdir(), 'btmf-'));
    const daemonDir = join(tmp, 'daemon');
    const imageDir = join(tmp, 'image');

    for (const [version, when] of [
      ['0.0.1', '2026-01-01T12:00:00Z'],
      ['0.1.0', '2026-02-15T18:00:00Z'],
      ['0.2.0', '2026-03-01T09:00:00Z'],
    ]) {
      await writeRelease(
        daemonDir,
        version!,
        { version, channel: 'stable', released_at: when, summary: `d ${version}` },
        `## ${version}`,
      );
    }
    await writeRelease(
      imageDir,
      '2026.01.0',
      { version: '2026.01.0', channel: 'stable', released_at: '2026-01-01T12:00:00Z', summary: 'i1' },
      '## i1',
    );

    const [daemonReleases, imageReleases] = await Promise.all([
      readComponentReleases(daemonDir),
      readComponentReleases(imageDir),
    ]);

    const yanked = generate({
      project,
      channels,
      daemonReleases,
      imageReleases,
      bundles: [
        bundle('0.0.1', '2026.01.0', 'stable', '2026-01-01T12:00:00Z', { imageBumped: true }),
        bundle('0.1.0', '2026.01.0', 'stable', '2026-02-15T18:00:00Z'),
        bundle('0.2.0', '2026.01.0', 'stable', '2026-03-01T09:00:00Z', { yanked: 'bricks wifi' }),
      ],
      updatedAt: '2026-03-01T09:00:01Z',
    });

    expect(yanked.channels['stable']!.latest).toBe('0.1.0+image.2026.01.0');
    expect(yanked.channels['stable']!.releases).toEqual([
      '0.2.0+image.2026.01.0',
      '0.1.0+image.2026.01.0',
      '0.0.1+image.2026.01.0',
    ]);

    const deprecated = generate({
      project,
      channels,
      daemonReleases,
      imageReleases,
      bundles: [
        bundle('0.0.1', '2026.01.0', 'stable', '2026-01-01T12:00:00Z', { imageBumped: true }),
        bundle('0.1.0', '2026.01.0', 'stable', '2026-02-15T18:00:00Z'),
        bundle('0.2.0', '2026.01.0', 'stable', '2026-03-01T09:00:00Z', { deprecated: true }),
      ],
      updatedAt: '2026-03-01T09:00:01Z',
    });

    expect(deprecated.channels['stable']!.latest).toBe('0.1.0+image.2026.01.0');
  });

  test('latest keeps the newest release when the whole channel is withdrawn', async () => {
    const tmp = await mkdtemp(join(tmpdir(), 'btmf-'));
    const daemonDir = join(tmp, 'daemon');
    const imageDir = join(tmp, 'image');

    await writeRelease(
      daemonDir,
      '0.1.0',
      { version: '0.1.0', channel: 'stable', released_at: '2026-02-15T18:00:00Z', summary: 'd' },
      '## d',
    );
    await writeRelease(
      imageDir,
      '2026.01.0',
      { version: '2026.01.0', channel: 'stable', released_at: '2026-01-01T12:00:00Z', summary: 'i' },
      '## i',
    );

    const [daemonReleases, imageReleases] = await Promise.all([
      readComponentReleases(daemonDir),
      readComponentReleases(imageDir),
    ]);

    const manifest = generate({
      project,
      channels,
      daemonReleases,
      imageReleases,
      bundles: [bundle('0.1.0', '2026.01.0', 'stable', '2026-02-15T18:00:00Z', { yanked: 'bad' })],
      updatedAt: '2026-02-15T18:00:01Z',
    });

    expect(manifest.channels['stable']!.latest).toBe('0.1.0+image.2026.01.0');
  });

  test('rejects bundle referencing unknown daemon version', async () => {
    const tmp = await mkdtemp(join(tmpdir(), 'btmf-'));
    const daemonDir = join(tmp, 'daemon');
    const imageDir = join(tmp, 'image');
    await writeRelease(
      imageDir,
      '2026.01.0',
      {
        version: '2026.01.0',
        channel: 'stable',
        released_at: '2026-01-01T12:00:00Z',
        summary: 'Initial image.',
      },
      '## i1',
    );

    const [daemonReleases, imageReleases] = await Promise.all([
      readComponentReleases(daemonDir),
      readComponentReleases(imageDir),
    ]);

    expect(() =>
      generate({
        project,
        channels,
        daemonReleases,
        imageReleases,
        bundles: [bundle('9.9.9', '2026.01.0', 'stable', '2026-02-15T18:00:00Z')],
        updatedAt: '2026-02-15T18:00:01Z',
      }),
    ).toThrow(/missing daemon release/);
  });

  test('skips channels with no bundles', async () => {
    const tmp = await mkdtemp(join(tmpdir(), 'btmf-'));
    const daemonDir = join(tmp, 'daemon');
    const imageDir = join(tmp, 'image');
    await writeRelease(
      daemonDir,
      '0.1.0',
      {
        version: '0.1.0',
        channel: 'stable',
        released_at: '2026-02-15T18:00:00Z',
        summary: 'd2',
      },
      '## d2',
    );
    await writeRelease(
      imageDir,
      '2026.01.0',
      {
        version: '2026.01.0',
        channel: 'stable',
        released_at: '2026-01-01T12:00:00Z',
        summary: 'i1',
      },
      '## i1',
    );

    const [daemonReleases, imageReleases] = await Promise.all([
      readComponentReleases(daemonDir),
      readComponentReleases(imageDir),
    ]);

    const manifest = generate({
      project,
      channels,
      daemonReleases,
      imageReleases,
      bundles: [bundle('0.1.0', '2026.01.0', 'stable', '2026-02-15T18:00:00Z')],
      updatedAt: '2026-02-15T18:00:01Z',
    });

    expect(manifest.channels['stable']).toBeTruthy();
    expect(manifest.channels['dev']).toBeUndefined();
  });
});
