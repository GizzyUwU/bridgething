import { describe, expect, test } from 'bun:test';
import { composeMeta, type FlashMeta } from './bundle-zip.ts';

function yoctoMeta(overrides: Partial<FlashMeta> = {}): FlashMeta {
  return {
    metadataVersion: 2,
    name: 'bridgething-prod-image',
    version: '0.1.7',
    description: 'Mainline u-boot first flash: signed FIP + wic + bandaid',
    steps: [
      { type: 'bulkcmd', value: 'amlmmc key' },
      { type: 'writeBootPartition', value: { hwpart: 1, data: { filePath: 'superbird-boot.bin' } } },
      { type: 'writeBootPartition', value: { hwpart: 2, data: { filePath: 'superbird-boot.bin' } } },
      { type: 'writeUserArea', value: { lba: 0, data: { filePath: 'superbird.wic' }, sparse: true } },
      { type: 'writeUserArea', value: { lba: 6389760, data: { filePath: 'bandaid.ext4' } } },
    ],
    ...overrides,
  };
}

const stable = { daemonVersion: '0.5.0', imageVersion: '0.1.7', channel: 'stable' };
const dev = { daemonVersion: '0.5.0', imageVersion: '0.1.7-dev', channel: 'dev' };

describe('composeMeta', () => {
  test('stamps the composite version the bundle actually is, not the image version', () => {
    expect(composeMeta(yoctoMeta(), stable).version).toBe('0.5.0+image.0.1.7');
    expect(composeMeta(yoctoMeta({ version: '0.1.7-dev' }), dev).version).toBe('0.5.0+image.0.1.7-dev');
  });

  test('names the bundle by channel', () => {
    expect(composeMeta(yoctoMeta(), stable).name).toBe('bridgething');
    expect(composeMeta(yoctoMeta(), dev).name).toBe('bridgething-dev');
  });

  test('describes both halves', () => {
    expect(composeMeta(yoctoMeta(), stable).description).toBe('bridgething 0.5.0 on image 0.1.7 (stable)');
  });

  test('leaves the steps and their lbas exactly as the bbclass rendered them', () => {
    const original = yoctoMeta();
    const composed = composeMeta(original, stable);
    expect(composed.steps).toEqual(original.steps);
    expect(composed.metadataVersion).toBe(2);
  });

  test('keeps fields the composer does not know about', () => {
    const composed = composeMeta(yoctoMeta({ variables: { slot: 1 } }), stable);
    expect(composed['variables']).toEqual({ slot: 1 });
  });

  test('refuses a metadata version it does not understand', () => {
    expect(() => composeMeta(yoctoMeta({ metadataVersion: 1 }), stable)).toThrow(/metadataVersion 1/);
  });

  test('refuses a package with no steps', () => {
    expect(() => composeMeta(yoctoMeta({ steps: [] }), stable)).toThrow(/no steps/);
  });
});
