export type ArtifactSource = {
  readonly size: number;
  read(offset: number, length: number): Promise<Uint8Array>;
  close?(): Promise<void> | void;
};

export async function readExact(source: ArtifactSource, offset: number, length: number): Promise<Uint8Array> {
  const first = await source.read(offset, length);
  if (first.byteLength === length) return first;
  if (first.byteLength === 0) throw new Error(`unexpected EOF at offset ${offset}, wanted ${length} bytes`);

  const out = new Uint8Array(length);
  out.set(first, 0);
  let got = first.byteLength;
  while (got < length) {
    const chunk = await source.read(offset + got, length - got);
    if (chunk.byteLength === 0) {
      throw new Error(`unexpected EOF at offset ${offset + got}, wanted ${length} bytes from ${offset}`);
    }
    out.set(chunk, got);
    got += chunk.byteLength;
  }
  return out;
}

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
  const bytes = await readExact(source, 0, source.size);
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
