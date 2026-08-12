const EOCD_SIGNATURE = 0x06054b50;
const EOCD_MIN_SIZE = 22;
const EOCD_MAX_SIZE = EOCD_MIN_SIZE + 0xffff;
const CENTRAL_SIGNATURE = 0x02014b50;

const METHOD_STORED = 0;
const METHOD_DEFLATED = 8;

export type ZipEntry = {
  name: string;
  method: number;
  compressedSize: number;
  uncompressedSize: number;
  localHeaderOffset: number;
};

export type EntryReader = {
  read(length: number): Promise<Uint8Array>;
  cancel(): Promise<void>;
};

export class ZipReader {
  private constructor(
    private readonly blob: Blob,
    private readonly entries: Map<string, ZipEntry>,
  ) {}

  static async open(blob: Blob): Promise<ZipReader> {
    const tailLength = Math.min(EOCD_MAX_SIZE, blob.size);
    const tail = new DataView(await blob.slice(blob.size - tailLength).arrayBuffer());

    let eocd = -1;
    for (let i = tail.byteLength - EOCD_MIN_SIZE; i >= 0; i--) {
      if (tail.getUint32(i, true) === EOCD_SIGNATURE) {
        eocd = i;
        break;
      }
    }
    if (eocd === -1) throw new Error('not a zip: no end-of-central-directory record');

    const count = tail.getUint16(eocd + 10, true);
    const directorySize = tail.getUint32(eocd + 12, true);
    const directoryOffset = tail.getUint32(eocd + 16, true);
    if (directoryOffset === 0xffffffff) throw new Error('zip64 archives are not supported');

    const directory = new DataView(await blob.slice(directoryOffset, directoryOffset + directorySize).arrayBuffer());

    const entries = new Map<string, ZipEntry>();
    const decoder = new TextDecoder();
    let cursor = 0;

    for (let i = 0; i < count; i++) {
      if (directory.getUint32(cursor, true) !== CENTRAL_SIGNATURE) {
        throw new Error(`corrupt central directory at entry ${i}`);
      }
      const nameLength = directory.getUint16(cursor + 28, true);
      const extraLength = directory.getUint16(cursor + 30, true);
      const commentLength = directory.getUint16(cursor + 32, true);
      const name = decoder.decode(new Uint8Array(directory.buffer, cursor + 46, nameLength));

      entries.set(name, {
        name,
        method: directory.getUint16(cursor + 10, true),
        compressedSize: directory.getUint32(cursor + 20, true),
        uncompressedSize: directory.getUint32(cursor + 24, true),
        localHeaderOffset: directory.getUint32(cursor + 42, true),
      });

      cursor += 46 + nameLength + extraLength + commentLength;
    }

    return new ZipReader(blob, entries);
  }

  names(): string[] {
    return [...this.entries.keys()];
  }

  entry(name: string): ZipEntry {
    const found = this.entries.get(name) ?? this.entries.get(name.replace(/^\.\//, ''));
    if (!found) throw new Error(`no such entry in bundle: ${name}`);
    return found;
  }

  size(name: string): number {
    return this.entry(name).uncompressedSize;
  }

  private async dataStart(entry: ZipEntry): Promise<number> {
    const header = new DataView(
      await this.blob.slice(entry.localHeaderOffset, entry.localHeaderOffset + 30).arrayBuffer(),
    );
    const nameLength = header.getUint16(26, true);
    const extraLength = header.getUint16(28, true);
    return entry.localHeaderOffset + 30 + nameLength + extraLength;
  }

  private async stream(name: string): Promise<ReadableStream<Uint8Array>> {
    const entry = this.entry(name);
    const start = await this.dataStart(entry);
    const raw = this.blob.slice(start, start + entry.compressedSize).stream();

    if (entry.method === METHOD_STORED) return raw;
    if (entry.method === METHOD_DEFLATED) return raw.pipeThrough(new DecompressionStream('deflate-raw'));
    throw new Error(`unsupported compression method ${entry.method} for ${entry.name}`);
  }

  async readAll(name: string): Promise<Uint8Array> {
    const out = new Uint8Array(this.size(name));
    const reader = (await this.stream(name)).getReader();
    let offset = 0;
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      out.set(value, offset);
      offset += value.length;
    }
    if (offset !== out.length) throw new Error(`short read on ${name}: ${offset} of ${out.length}`);
    return out;
  }

  async open(name: string): Promise<{ size: number; reader: EntryReader }> {
    const reader = (await this.stream(name)).getReader();
    let pending: Uint8Array = new Uint8Array(0);
    let exhausted = false;

    return {
      size: this.size(name),
      reader: {
        async read(length: number): Promise<Uint8Array> {
          while (pending.length < length && !exhausted) {
            const { done, value } = await reader.read();
            if (done) {
              exhausted = true;
              break;
            }
            const grown = new Uint8Array(pending.length + value.length);
            grown.set(pending);
            grown.set(value, pending.length);
            pending = grown;
          }
          if (pending.length < length) throw new Error(`unexpected end of ${name}`);
          const out = pending.subarray(0, length);
          pending = pending.subarray(length);
          return out;
        },
        async cancel() {
          await reader.cancel().catch(() => {});
        },
      },
    };
  }
}
