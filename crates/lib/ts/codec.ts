import { decode as msgpackDecode, encode as msgpackEncode } from '@msgpack/msgpack';
import { gzip, ungzip } from 'pako';
import type { Priority } from './bindings/shared';
import { uuidFromString, uuidToString } from './uuid';
import { UUID_FIELD_NAMES } from './uuid-fields.generated';

export const Compression = {
  None: 0x00,
  Gzip: 0x01,
} as const;
export type Compression = (typeof Compression)[keyof typeof Compression];

export const Encoding = {
  Msgpack: 0x00,
  Json: 0x01,
} as const;
export type Encoding = (typeof Encoding)[keyof typeof Encoding];

export const FRAME_HEADER_LENGTH = 16;
export const FRAME_MAGIC = 0xdead;
export const FRAME_VERSION = 2;

export const priorityToByte = (p: Priority): number => (p === 'bulk' ? 0x01 : 0x00);
export const priorityFromByte = (b: number): Priority => (b === 0x01 ? 'bulk' : 'normal');

export class CodecError extends Error {
  constructor(
    message: string,
    public readonly kind:
      | 'header-too-short'
      | 'invalid-magic'
      | 'unsupported-version'
      | 'unsupported-compression'
      | 'unsupported-encoding'
      | 'payload-too-short',
  ) {
    super(message);
    this.name = 'CodecError';
  }
}

/**
 * Wire header: `magic u16 BE | version u8 | compression u8 | encoding u8 |
 * priority u8 | reserved [2]u8 | length u64 BE`. Total 16 bytes.
 */
export type FrameHeader = {
  compression: Compression;
  encoding: Encoding;
  priority: Priority;
  payloadLength: number;
};

export function writeFrameHeader(header: FrameHeader): Uint8Array {
  const buf = new Uint8Array(FRAME_HEADER_LENGTH);
  const view = new DataView(buf.buffer);
  view.setUint16(0, FRAME_MAGIC, false);
  view.setUint8(2, FRAME_VERSION);
  view.setUint8(3, header.compression);
  view.setUint8(4, header.encoding);
  view.setUint8(5, priorityToByte(header.priority));
  // bytes 6..8 reserved zero
  // u64 BE length - JS can't address > 2^53 but BigInt path keeps the wire honest.
  view.setBigUint64(8, BigInt(header.payloadLength), false);
  return buf;
}

export function parseFrameHeader(frame: Uint8Array): FrameHeader {
  if (frame.length < FRAME_HEADER_LENGTH) {
    throw new CodecError(`frame shorter than header (${frame.length})`, 'header-too-short');
  }
  const view = new DataView(frame.buffer, frame.byteOffset, frame.byteLength);
  const magic = view.getUint16(0, false);
  if (magic !== FRAME_MAGIC) throw new CodecError(`bad magic 0x${magic.toString(16)}`, 'invalid-magic');
  const version = view.getUint8(2);
  if (version !== FRAME_VERSION) throw new CodecError(`unsupported version ${version}`, 'unsupported-version');
  const compression = view.getUint8(3) as Compression;
  if (compression !== Compression.None && compression !== Compression.Gzip) {
    throw new CodecError(`unsupported compression ${compression}`, 'unsupported-compression');
  }
  const encoding = view.getUint8(4) as Encoding;
  if (encoding !== Encoding.Msgpack && encoding !== Encoding.Json) {
    throw new CodecError(`unsupported encoding ${encoding}`, 'unsupported-encoding');
  }
  const priority = priorityFromByte(view.getUint8(5));
  const length = Number(view.getBigUint64(8, false));
  return { compression, encoding, priority, payloadLength: length };
}

export type CodecOptions = {
  compression?: Compression;
  encoding?: Encoding;
  priority?: Priority;
};

const UUID_REGEX = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/**
 * Lazy recursive walk that converts UUID-typed fields between their
 * msgpack on-wire form (16-byte `bin`) and their SDK-surface form
 * (hyphenated string). Field names come from the codegen-emitted
 * `UUID_FIELD_NAMES` set.
 *
 * The walk only allocates a new container when something changes, so
 * payloads with no UUID fields pass through with no overhead.
 */
export function walkUuidFields(value: unknown, mode: 'decode' | 'encode'): unknown {
  if (value === null || typeof value !== 'object') return value;
  if (value instanceof Uint8Array) return value;
  if (Array.isArray(value)) {
    let cloned: unknown[] | null = null;
    for (let i = 0; i < value.length; i++) {
      const next = walkUuidFields(value[i], mode);
      if (next !== value[i] && cloned === null) cloned = value.slice();
      if (cloned) cloned[i] = next;
    }
    return cloned ?? value;
  }
  const record = value as Record<string, unknown>;
  let cloned: Record<string, unknown> | null = null;
  for (const key of Object.keys(record)) {
    const v = record[key];
    let next: unknown = v;
    if (UUID_FIELD_NAMES.has(key)) {
      if (mode === 'decode' && v instanceof Uint8Array && v.length === 16) {
        next = uuidToString(v);
      } else if (mode === 'encode' && typeof v === 'string' && UUID_REGEX.test(v)) {
        next = uuidFromString(v);
      } else {
        next = walkUuidFields(v, mode);
      }
    } else {
      next = walkUuidFields(v, mode);
    }
    if (next !== v) {
      if (cloned === null) cloned = { ...record };
      cloned[key] = next;
    }
  }
  return cloned ?? record;
}

/**
 * Encode/decode bridgething wire messages.
 *
 * Encode: `T` → msgpack/json (encoding) → gzip/raw (compression) → 16-byte header + body.
 * Decode: header → body → gunzip/raw → msgpack/json → `T`.
 *
 * UUIDs travel as 16-byte msgpack `bin` on the gateway path and as
 * hyphenated strings on JSON. The codec exposes both as strings to the
 * SDK consumer; `walkUuidFields` does the bin ↔ string conversion at
 * codegen-known field names on the msgpack side. JSON is a no-op.
 */
export class Codec {
  readonly compression: Compression;
  readonly encoding: Encoding;

  constructor(options: CodecOptions = {}) {
    this.compression = options.compression ?? Compression.None;
    this.encoding = options.encoding ?? Encoding.Msgpack;
  }

  encode<T>(message: T, overrides: CodecOptions = {}): Uint8Array {
    const compression = overrides.compression ?? this.compression;
    const encoding = overrides.encoding ?? this.encoding;
    const priority: Priority = overrides.priority ?? 'normal';

    const wirePayload = encoding === Encoding.Msgpack ? walkUuidFields(message, 'encode') : message;
    const payload =
      encoding === Encoding.Msgpack
        ? msgpackEncode(wirePayload)
        : new TextEncoder().encode(JSON.stringify(wirePayload));
    const body = compression === Compression.Gzip ? gzip(payload) : payload;

    const header = writeFrameHeader({ compression, encoding, priority, payloadLength: body.length });
    const out = new Uint8Array(header.length + body.length);
    out.set(header, 0);
    out.set(body, header.length);
    return out;
  }

  decode<T>(frame: Uint8Array): T {
    const header = parseFrameHeader(frame);
    const total = FRAME_HEADER_LENGTH + header.payloadLength;
    if (frame.length < total) {
      throw new CodecError(`payload truncated (have ${frame.length}, need ${total})`, 'payload-too-short');
    }
    const body = frame.subarray(FRAME_HEADER_LENGTH, total);
    const payload = header.compression === Compression.Gzip ? ungzip(body) : body;
    if (header.encoding === Encoding.Msgpack) {
      return walkUuidFields(msgpackDecode(payload), 'decode') as T;
    }
    return JSON.parse(new TextDecoder().decode(payload)) as T;
  }
}
