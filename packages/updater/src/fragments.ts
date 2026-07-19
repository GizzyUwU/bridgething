import type { Priority, RangePart } from '@bridgething/lib';

import type { AckWindow } from './ack-window.js';
import { TransferStalledError } from './ack-window.js';
import type { GatewayDevice } from './gateway-device.js';
import type { ArtifactSource } from './source.js';

export const DEFAULT_FRAGMENT_BYTES = 64 * 1024;

export type StreamSourceOptions = {
  device: GatewayDevice;
  transferId: string;
  source: ArtifactSource;
  startOffset: number;
  totalSize: number;
  chunkSize: number;
  priority: Priority;
  window: AckWindow;
  signal?: AbortSignal;
};

export async function streamSourceFragments(opts: StreamSourceOptions): Promise<void> {
  let offset = opts.startOffset;
  while (offset < opts.totalSize) {
    if (opts.signal?.aborted) return;
    const ok = await opts.window.waitForRoom(offset);
    if (opts.signal?.aborted) return;
    if (!ok) throw new TransferStalledError(offset, opts.totalSize);

    const want = Math.min(opts.chunkSize, opts.totalSize - offset);
    const bytes = await opts.source.read(offset, want);
    if (bytes.byteLength === 0) {
      throw new Error(`unexpected EOF at offset ${offset}/${opts.totalSize}`);
    }
    await opts.device.transfer.fragment({ transferId: opts.transferId, offset, bytes }, { priority: opts.priority });
    offset += bytes.byteLength;
  }
}

export type StreamRangeOptions = {
  device: GatewayDevice;
  transferId: string;
  source: ArtifactSource;
  ranges: RangePart[];
  chunkSize: number;
  priority: Priority;
  window: AckWindow;
  signal?: AbortSignal;
};

export async function streamRangeFragments(opts: StreamRangeOptions): Promise<void> {
  let streamOffset = 0;
  for (const range of opts.ranges) {
    let produced = 0;
    while (produced < range.length) {
      if (opts.signal?.aborted) return;
      const ok = await opts.window.waitForRoom(streamOffset);
      if (opts.signal?.aborted) return;
      if (!ok) throw new TransferStalledError(streamOffset, streamOffset + (range.length - produced));

      const want = Math.min(opts.chunkSize, range.length - produced);
      const bytes = await opts.source.read(range.start + produced, want);
      if (bytes.byteLength === 0) {
        throw new Error(`unexpected EOF reading range at offset ${range.start + produced}`);
      }
      await opts.device.transfer.fragment(
        { transferId: opts.transferId, offset: streamOffset, bytes },
        { priority: opts.priority },
      );
      produced += bytes.byteLength;
      streamOffset += bytes.byteLength;
    }
  }
}
