import { open } from 'node:fs/promises';

import type { ArtifactSource } from './source.js';

export async function fileArtifactSource(path: string): Promise<ArtifactSource> {
  const handle = await open(path, 'r');
  const stat = await handle.stat();
  return {
    size: stat.size,
    async read(offset, length) {
      const buf = new Uint8Array(length);
      const { bytesRead } = await handle.read(buf, 0, length, offset);
      return buf.subarray(0, bytesRead);
    },
    close: () => handle.close(),
  };
}
