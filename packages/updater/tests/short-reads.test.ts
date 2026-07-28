import { describe, expect, test } from 'bun:test';

import type { RangePart } from '@bridgething/lib';

import { AckRegistry, AckWindow } from '../src/ack-window';
import { streamRangeFragments } from '../src/fragments';
import type { GatewayDevice } from '../src/gateway-device';
import { serveOtaAssetRanges } from '../src/range-serve';
import { bytesArtifactSource, sha256Hex, type ArtifactSource } from '../src/source';

const artifact = (size: number) => new Uint8Array(size).map((_, i) => (i * 11 + (i >> 5)) & 0xff);

function shortReading(bytes: Uint8Array, most: number): ArtifactSource {
  return {
    size: bytes.byteLength,
    read(offset, length) {
      const take = Math.min(length, most, bytes.byteLength - offset);
      return Promise.resolve(bytes.subarray(offset, offset + Math.max(0, take)));
    },
  };
}

type Responded = {
  totalSize: number;
  parts: RangePart[];
  body: { type: string; data: unknown };
};

function rangeServer(source: ArtifactSource) {
  let handler: ((handle: unknown, req: unknown) => Promise<void> | void) | null = null;
  const sent: Array<{ offset: number; bytes: Uint8Array }> = [];
  let responded: Responded | null = null;
  let errored: string | null = null;

  const device = {
    system: {
      onOtaAssetRange(h: (handle: unknown, req: unknown) => Promise<void> | void) {
        handler = h;
        return () => {};
      },
    },
    transfer: {
      fragment(frame: { offset: number; bytes: Uint8Array }) {
        sent.push({ offset: frame.offset, bytes: frame.bytes.slice() });
        return Promise.resolve();
      },
    },
  } as unknown as GatewayDevice;

  const registry = new AckRegistry();
  serveOtaAssetRanges(device, registry, new Map([['image.zck', source]]));

  const handle = {
    requestId: 'req-1',
    respond: (r: Responded) => {
      responded = r;
      return Promise.resolve();
    },
    respondErr: (e: { reason: string }) => {
      errored = e.reason;
      return Promise.resolve();
    },
  };

  return {
    async request(ranges: RangePart[]) {
      const window = registry.register('req-1', 0);
      const tick = setInterval(() => window.note(window.ackedBytes + 64 * 1024), 1);
      await handler?.(handle, { asset: 'image.zck', ranges });
      clearInterval(tick);
      return { responded, errored, sent };
    },
  };
}

const SIZE = 8_000;

describe('a source that short-reads', () => {
  test('still hashes the whole artifact', async () => {
    const bytes = artifact(SIZE);
    const whole = await sha256Hex(bytesArtifactSource(bytes));

    expect(await sha256Hex(shortReading(bytes, 512))).toBe(whole);
  });

  test('still serves a complete inline range', async () => {
    const bytes = artifact(SIZE);
    const server = rangeServer(shortReading(bytes, 100));

    const { responded, errored } = await server.request([{ start: 0, length: 4096 }]);

    expect(errored).toBeNull();
    expect(responded?.body.type).toBe('inline');
    expect(responded?.body.data).toEqual(bytes.subarray(0, 4096));
  });

  test('still streams a complete oversized range', async () => {
    const bytes = artifact(SIZE);
    const window = new AckWindow(0, { windowBytes: 1 << 20, ackTimeoutMs: 1000 });
    const sent: Array<{ offset: number; bytes: Uint8Array }> = [];
    const device = {
      transfer: {
        fragment(frame: { offset: number; bytes: Uint8Array }) {
          sent.push({ offset: frame.offset, bytes: frame.bytes.slice() });
          return Promise.resolve();
        },
      },
    } as unknown as GatewayDevice;

    await streamRangeFragments({
      device,
      transferId: 't',
      source: shortReading(bytes, 100),
      ranges: [{ start: 0, length: 4096 }],
      chunkSize: 1024,
      priority: 'background',
      window,
    });

    const out = new Uint8Array(4096);
    for (const s of sent) out.set(s.bytes, s.offset);
    expect(out).toEqual(bytes.subarray(0, 4096));
  });
});
