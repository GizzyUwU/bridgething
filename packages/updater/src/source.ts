export type ArtifactSource = {
  readonly size: number;
  read(offset: number, length: number): Promise<Uint8Array>;
  close?(): Promise<void> | void;
};

export function bytesArtifactSource(bytes: Uint8Array): ArtifactSource {
  return {
    size: bytes.byteLength,
    read(offset, length) {
      return Promise.resolve(bytes.subarray(offset, offset + length));
    },
  };
}

export function blobArtifactSource(blob: Blob): ArtifactSource {
  return {
    size: blob.size,
    async read(offset, length) {
      const slice = blob.slice(offset, offset + length);
      return new Uint8Array(await slice.arrayBuffer());
    },
  };
}

export async function sha256Hex(source: ArtifactSource): Promise<string> {
  const bytes = await source.read(0, source.size);
  const digest = await crypto.subtle.digest('SHA-256', toArrayBuffer(bytes));
  return Array.from(new Uint8Array(digest))
    .map(b => b.toString(16).padStart(2, '0'))
    .join('');
}

function toArrayBuffer(bytes: Uint8Array): ArrayBuffer {
  if (bytes.buffer instanceof ArrayBuffer && bytes.byteOffset === 0 && bytes.byteLength === bytes.buffer.byteLength) {
    return bytes.buffer;
  }
  const out = new ArrayBuffer(bytes.byteLength);
  new Uint8Array(out).set(bytes);
  return out;
}
