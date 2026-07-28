import { describe, expect, test } from 'bun:test';

import type { RangePart } from '@bridgething/lib';

import { AckWindow } from '../src/ack-window';
import { streamRangeFragments, streamSourceFragments } from '../src/fragments';
import type { GatewayDevice } from '../src/gateway-device';
import { bytesArtifactSource } from '../src/source';

type Sent = { offset: number; bytes: Uint8Array };

function receiver(window: AckWindow) {
  const sent: Sent[] = [];
  const device = {
    transfer: {
      fragment(frame: { transferId: string; offset: number; bytes: Uint8Array }) {
        sent.push({ offset: frame.offset, bytes: frame.bytes.slice() });
        window.note(frame.offset + frame.bytes.byteLength);
        return Promise.resolve();
      },
    },
  } as unknown as GatewayDevice;
  return { device, sent };
}

function reassemble(sent: Sent[], from = 0): Uint8Array {
  const end = sent.reduce((max, s) => Math.max(max, s.offset + s.bytes.byteLength), 0);
  const out = new Uint8Array(end);
  const written = new Uint8Array(end);
  for (const s of sent) {
    out.set(s.bytes, s.offset);
    for (let i = 0; i < s.bytes.byteLength; i++) written[s.offset + i] += 1;
  }
  const bad = Array.from(written.subarray(from)).filter(count => count !== 1).length;
  if (bad > 0) throw new Error(`${bad} bytes of the stream were written other than exactly once`);
  return out;
}

const artifact = (size: number) => new Uint8Array(size).map((_, i) => (i * 7 + (i >> 8)) & 0xff);

const expected = (bytes: Uint8Array, ranges: RangePart[]) => {
  const out = new Uint8Array(ranges.reduce((sum, r) => sum + r.length, 0));
  let at = 0;
  for (const r of ranges) {
    out.set(bytes.subarray(r.start, r.start + r.length), at);
    at += r.length;
  }
  return out;
};

async function streamRanges(size: number, ranges: RangePart[], chunkSize: number) {
  const window = new AckWindow(0, { windowBytes: 4096, ackTimeoutMs: 1000 });
  const { device, sent } = receiver(window);
  await streamRangeFragments({
    device,
    transferId: 't',
    source: bytesArtifactSource(artifact(size)),
    ranges,
    chunkSize,
    priority: 'background',
    window,
  });
  return { sent, bytes: reassemble(sent) };
}

const SIZE = 10_000;

describe('range fragments', () => {
  const cases: Array<{ what: string; ranges: RangePart[]; chunk: number }> = [
    { what: 'one range, chunk divides evenly', ranges: [{ start: 0, length: 4096 }], chunk: 1024 },
    { what: 'one range, chunk leaves a remainder', ranges: [{ start: 0, length: 5000 }], chunk: 1024 },
    { what: 'chunk larger than the range', ranges: [{ start: 100, length: 50 }], chunk: 4096 },
    { what: 'a single byte', ranges: [{ start: 4095, length: 1 }], chunk: 1024 },
    { what: 'a range ending exactly at the artifact end', ranges: [{ start: SIZE - 500, length: 500 }], chunk: 128 },
    {
      what: 'several disjoint ranges',
      ranges: [
        { start: 0, length: 300 },
        { start: 5000, length: 1200 },
        { start: 9000, length: 1000 },
      ],
      chunk: 256,
    },
    {
      what: 'overlapping ranges, which zchunk may legitimately ask for',
      ranges: [
        { start: 0, length: 2000 },
        { start: 1000, length: 2000 },
      ],
      chunk: 512,
    },
    {
      what: 'ranges walking backwards through the artifact',
      ranges: [
        { start: 8000, length: 500 },
        { start: 100, length: 500 },
      ],
      chunk: 300,
    },
  ];

  for (const c of cases) {
    test(`delivers the requested bytes: ${c.what}`, async () => {
      const source = artifact(SIZE);
      const { bytes } = await streamRanges(SIZE, c.ranges, c.chunk);

      expect(bytes).toEqual(expected(source, c.ranges));
    });
  }

  test('never emits a fragment larger than the chunk size', async () => {
    const { sent } = await streamRanges(SIZE, [{ start: 0, length: 5000 }], 1024);

    expect(sent.every(s => s.bytes.byteLength <= 1024)).toBe(true);
  });

  test('stops emitting once aborted', async () => {
    const controller = new AbortController();
    const window = new AckWindow(0, { windowBytes: 4096, ackTimeoutMs: 1000 });
    const { device, sent } = receiver(window);
    controller.abort();

    await streamRangeFragments({
      device,
      transferId: 't',
      source: bytesArtifactSource(artifact(SIZE)),
      ranges: [{ start: 0, length: 5000 }],
      chunkSize: 1024,
      priority: 'background',
      window,
      signal: controller.signal,
    });

    expect(sent).toEqual([]);
  });
});

describe('whole-source fragments', () => {
  test('delivers every byte of the artifact exactly once', async () => {
    const source = artifact(SIZE);
    const window = new AckWindow(0, { windowBytes: 4096, ackTimeoutMs: 1000 });
    const { device, sent } = receiver(window);

    await streamSourceFragments({
      device,
      transferId: 't',
      source: bytesArtifactSource(source),
      startOffset: 0,
      totalSize: SIZE,
      chunkSize: 1024,
      priority: 'background',
      window,
    });

    expect(reassemble(sent)).toEqual(source);
  });

  test('resuming starts at the offset it was given', async () => {
    const source = artifact(SIZE);
    const window = new AckWindow(6000, { windowBytes: 4096, ackTimeoutMs: 1000 });
    const { device, sent } = receiver(window);

    await streamSourceFragments({
      device,
      transferId: 't',
      source: bytesArtifactSource(source),
      startOffset: 6000,
      totalSize: SIZE,
      chunkSize: 1024,
      priority: 'background',
      window,
    });

    expect(sent[0]?.offset).toBe(6000);
    expect(reassemble(sent, 6000).subarray(6000)).toEqual(source.subarray(6000));
  });
});
