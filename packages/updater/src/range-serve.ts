import type { RangePart } from '@bridgething/lib';

import type { AckRegistry } from './ack-window.js';
import { streamRangeFragments } from './fragments.js';
import type { GatewayDevice } from './gateway-device.js';
import { readExact, type ArtifactSource } from './source.js';

const RANGE_INLINE_MAX_BYTES = 16 * 1024;
const RANGE_CHUNK_BYTES = 64 * 1024;

export function serveOtaAssetRanges(
  device: GatewayDevice,
  registry: AckRegistry,
  zcks: Map<string, ArtifactSource>,
): () => void {
  return device.system.onOtaAssetRange(async (handle, req) => {
    const source = zcks.get(req.asset);
    if (!source) {
      await handle.respondErr({ reason: `no local .zck for asset ${req.asset}` });
      return;
    }

    for (const r of req.ranges) {
      if (r.start + r.length > source.size) {
        await handle.respondErr({ reason: `range ${r.start}+${r.length} exceeds zck size ${source.size}` });
        return;
      }
    }

    const parts: RangePart[] = req.ranges.map(r => ({ start: r.start, length: r.length }));
    const streamLen = parts.reduce((sum, p) => sum + p.length, 0);

    if (streamLen <= RANGE_INLINE_MAX_BYTES) {
      const pieces: Uint8Array[] = [];
      for (const part of parts) pieces.push(await readExact(source, part.start, part.length));
      await handle.respond({ totalSize: source.size, parts, body: { type: 'inline', data: concat(pieces) } });
      return;
    }

    await handle.respond({
      totalSize: source.size,
      parts,
      body: { type: 'stream', data: { id: handle.requestId, totalSize: streamLen, sha256: null } },
    });

    const window = registry.register(handle.requestId, 0);
    try {
      await streamRangeFragments({
        device,
        transferId: handle.requestId,
        source,
        ranges: parts,
        chunkSize: RANGE_CHUNK_BYTES,
        priority: 'background',
        window,
      });
    } finally {
      registry.deregister(handle.requestId);
    }
  });
}

function concat(pieces: Uint8Array[]): Uint8Array {
  const total = pieces.reduce((sum, p) => sum + p.byteLength, 0);
  const out = new Uint8Array(total);
  let offset = 0;
  for (const piece of pieces) {
    out.set(piece, offset);
    offset += piece.byteLength;
  }
  return out;
}
