import { describe, expect, test } from 'bun:test';
import { mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { parse } from 'yaml';

const SCRIPT = resolve(import.meta.dirname, 'update-bundles-yaml.ts');

const BASE = [
  '--channel',
  'stable',
  '--daemon-version',
  '0.8.0',
  '--image-version',
  '0.1.11',
  '--daemon-bumped',
  'true',
  '--image-bumped',
  'false',
  '--size',
  '1024',
  '--sha256',
  'a'.repeat(64),
  '--url',
  'https://apps.example/r/bundle.zip',
];

async function run(extra: string[]) {
  const dir = await mkdtemp(join(tmpdir(), 'btbundles-'));
  const bundlesPath = join(dir, 'bundles.yaml');
  await writeFile(bundlesPath, 'bundles: []\n');

  const proc = Bun.spawn(['bun', 'run', SCRIPT, ...BASE, '--bundles-path', bundlesPath, ...extra], {
    stdout: 'pipe',
    stderr: 'pipe',
  });
  const [code, stderr] = await Promise.all([proc.exited, new Response(proc.stderr).text()]);
  const doc = parse(await readFile(bundlesPath, 'utf-8')) as { bundles: Record<string, unknown>[] };
  return { code, stderr, entry: doc.bundles[0] };
}

describe('wakeword', () => {
  test('writes the block the release schema requires', async () => {
    const { code, entry } = await run([
      '--wakeword-runtime',
      '0.8.0',
      '--wakeword-model',
      '1.2.0',
      '--wakeword-trained-against',
      '1.2.0=0.8.0',
      '--wakeword-trained-against',
      '1.1.0=0.7.0',
      '--wakeword-artifact',
      `model=1310720:${'b'.repeat(64)}`,
    ]);

    expect(code).toBe(0);
    expect(entry!['wakeword']).toEqual({
      runtime: '0.8.0',
      model: '1.2.0',
      model_trained_against: { '1.2.0': '0.8.0', '1.1.0': '0.7.0' },
    });
    expect(entry!['artifacts']).toEqual({ wakeword: { model: { size: 1310720, sha256: 'b'.repeat(64) } } });
  });

  test('omits the key entirely when the release declares no model', async () => {
    const { code, entry } = await run([]);

    expect(code).toBe(0);
    expect('wakeword' in entry!).toBe(false);
  });

  test('model_trained_against is dropped rather than emitted empty', async () => {
    const { code, entry } = await run(['--wakeword-runtime', '0.8.0', '--wakeword-model', '1.2.0']);

    expect(code).toBe(0);
    expect(entry!['wakeword']).toEqual({ runtime: '0.8.0', model: '1.2.0' });
  });

  test('refuses a model with no runtime', async () => {
    const { code, stderr } = await run(['--wakeword-model', '1.2.0']);

    expect(code).not.toBe(0);
    expect(stderr).toContain('must be given together');
  });

  test('refuses a runtime with no model', async () => {
    const { code, stderr } = await run(['--wakeword-runtime', '0.8.0']);

    expect(code).not.toBe(0);
    expect(stderr).toContain('must be given together');
  });

  test('refuses a trained-against map with no wakeword block to hang it on', async () => {
    const { code, stderr } = await run(['--wakeword-trained-against', '1.2.0=0.8.0']);

    expect(code).not.toBe(0);
    expect(stderr).toContain('--wakeword-trained-against needs');
  });

  test('refuses an artifact key that is not a wakeword artifact', async () => {
    const { code, stderr } = await run(['--wakeword-artifact', `phrase=10:${'c'.repeat(64)}`]);

    expect(code).not.toBe(0);
    expect(stderr).toContain('--wakeword-artifact key must be one of');
  });
});
