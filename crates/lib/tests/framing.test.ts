import { describe, expect, test } from 'bun:test';
import {
  Codec,
  CodecError,
  Compression,
  Encoding,
  FRAME_HEADER_LENGTH,
  FRAME_MAGIC,
  FRAME_VERSION,
  parseFrameHeader,
  writeFrameHeader,
} from '../ts/codec';
import { FrameAccumulator, FrameTooLargeError } from '../ts/framing';

const codec = new Codec({ compression: Compression.None, encoding: Encoding.Msgpack });

function frameOfPayload(payload: Uint8Array): Uint8Array {
  const header = writeFrameHeader({
    compression: Compression.None,
    encoding: Encoding.Msgpack,
    payloadLength: payload.length,
  });
  const out = new Uint8Array(header.length + payload.length);
  out.set(header, 0);
  out.set(payload, header.length);
  return out;
}

describe('FrameHeader', () => {
  test('round-trips magic, version, compression, encoding, length', () => {
    const header = writeFrameHeader({
      compression: Compression.Gzip,
      encoding: Encoding.Json,
      payloadLength: 0xdeadbeef,
    });
    expect(header.length).toBe(FRAME_HEADER_LENGTH);
    const view = new DataView(header.buffer);
    expect(view.getUint16(0, false)).toBe(FRAME_MAGIC);
    expect(view.getUint8(2)).toBe(FRAME_VERSION);
    expect(view.getUint8(3)).toBe(Compression.Gzip);
    expect(view.getUint8(4)).toBe(Encoding.Json);
    // reserved bytes 5..8 must be zero
    expect(view.getUint8(5)).toBe(0);
    expect(view.getUint8(6)).toBe(0);
    expect(view.getUint8(7)).toBe(0);

    const parsed = parseFrameHeader(header);
    expect(parsed.compression).toBe(Compression.Gzip);
    expect(parsed.encoding).toBe(Encoding.Json);
    expect(parsed.payloadLength).toBe(0xdeadbeef);
  });

  test('rejects bad magic', () => {
    const bad = writeFrameHeader({ compression: Compression.None, encoding: Encoding.Msgpack, payloadLength: 0 });
    bad[0] = 0;
    bad[1] = 0;
    expect(() => parseFrameHeader(bad)).toThrow(CodecError);
  });

  test('rejects unsupported version', () => {
    const bad = writeFrameHeader({ compression: Compression.None, encoding: Encoding.Msgpack, payloadLength: 0 });
    bad[2] = 99;
    expect(() => parseFrameHeader(bad)).toThrow(CodecError);
  });

  test('rejects short header', () => {
    expect(() => parseFrameHeader(new Uint8Array(8))).toThrow(CodecError);
  });
});

describe('FrameAccumulator', () => {
  test('returns null until full header is buffered', () => {
    const acc = new FrameAccumulator();
    acc.append(new Uint8Array([0xde, 0xad]));
    expect(acc.nextFrame()).toBeNull();
    expect(acc.bufferedByteCount).toBe(2);
  });

  test('returns null until full payload is buffered', () => {
    const acc = new FrameAccumulator();
    const frame = frameOfPayload(new Uint8Array(100).fill(0x42));
    acc.append(frame.subarray(0, FRAME_HEADER_LENGTH + 50));
    expect(acc.nextFrame()).toBeNull();
    acc.append(frame.subarray(FRAME_HEADER_LENGTH + 50));
    const popped = acc.nextFrame();
    expect(popped).not.toBeNull();
    expect(popped!.length).toBe(frame.length);
  });

  test('reassembles two frames split across many chunks', () => {
    const a = frameOfPayload(new Uint8Array([1, 2, 3, 4]));
    const b = frameOfPayload(new Uint8Array([5, 6, 7, 8, 9]));
    const stream = new Uint8Array(a.length + b.length);
    stream.set(a, 0);
    stream.set(b, a.length);

    const acc = new FrameAccumulator();
    for (const byte of stream) acc.append(new Uint8Array([byte]));

    const first = acc.nextFrame();
    const second = acc.nextFrame();
    expect(first).toEqual(a);
    expect(second).toEqual(b);
    expect(acc.nextFrame()).toBeNull();
  });

  test('throws on bad magic mid-stream', () => {
    const acc = new FrameAccumulator();
    const garbage = new Uint8Array(FRAME_HEADER_LENGTH);
    acc.append(garbage);
    expect(() => acc.nextFrame()).toThrow(CodecError);
  });

  test('throws on oversized payload', () => {
    const acc = new FrameAccumulator(64);
    acc.append(
      writeFrameHeader({
        compression: Compression.None,
        encoding: Encoding.Msgpack,
        payloadLength: 65,
      }),
    );
    expect(() => acc.nextFrame()).toThrow(FrameTooLargeError);
  });

  test('frames pop are independent of subsequent appends', () => {
    const a = frameOfPayload(new Uint8Array([0xaa, 0xbb, 0xcc]));
    const acc = new FrameAccumulator();
    acc.append(a);
    const popped = acc.nextFrame()!;
    acc.append(new Uint8Array([0xff, 0xff, 0xff]));
    // Mutating the buffer (via append + future nextFrame attempt) should not
    // change the bytes the caller already received.
    expect(popped[0]).toBe(0xde);
    expect(popped[1]).toBe(0xad);
  });
});

describe('Codec', () => {
  test('encode → decode round-trips a simple object via msgpack/no-compression', () => {
    const c = new Codec({ compression: Compression.None, encoding: Encoding.Msgpack });
    const message = { a: 1, b: 'hello', c: new Uint8Array([1, 2, 3]) };
    const frame = c.encode(message);
    const decoded = c.decode<typeof message>(frame);
    expect(decoded.a).toBe(1);
    expect(decoded.b).toBe('hello');
    expect(Array.from(decoded.c)).toEqual([1, 2, 3]);
  });

  test('encode → decode round-trips through gzip', () => {
    const c = new Codec({ compression: Compression.Gzip, encoding: Encoding.Msgpack });
    const message = { repeat: 'x'.repeat(2048) };
    const frame = c.encode(message);
    expect(frame.length).toBeLessThan(2048); // gzip kicks in
    const decoded = c.decode<typeof message>(frame);
    expect(decoded.repeat).toBe(message.repeat);
  });

  test('decode honours per-frame encoding/compression byte regardless of default', () => {
    const json = new Codec({ compression: Compression.None, encoding: Encoding.Json });
    const frame = json.encode({ hello: 'world' });
    // codec instance configured for msgpack still decodes the json frame
    // because we read the header byte.
    const decoded = codec.decode<{ hello: string }>(frame);
    expect(decoded.hello).toBe('world');
  });
});
