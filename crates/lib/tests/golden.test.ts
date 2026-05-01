import { describe, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import type { BridgeToGatewayMsg, GatewayToBridgeMsg } from '../ts/bindings/gateway';
import { Codec, Compression, Encoding, parseFrameHeader } from '../ts/codec';
import { uuidToString } from '../ts/uuid';

type Direction = 'bridge_to_gateway' | 'gateway_to_bridge';
type Fixture = {
  name: string;
  description: string;
  direction: Direction;
  priority: 'normal' | 'bulk';
  decoded_json: unknown;
  msgpack_hex: string;
  framed_hex: string;
};

type GoldenFile = {
  version: number;
  magic: string;
  fixtures: Fixture[];
};

function hexToBytes(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return out;
}

/**
 * Translate a decoded msgpack value (Uint8Array UUIDs, Uint8Array binaries)
 * into the JSON-friendly shape used in `decoded_json` (UUID strings, arrays
 * of bytes for the Forward/Image binary variants).
 */
function normalize(value: unknown, key: string): unknown {
  if (value instanceof Uint8Array) {
    if (value.length === 16 && (key === 'id' || key === 'requestId')) {
      return uuidToString(value);
    }
    return Array.from(value);
  }
  if (Array.isArray(value)) {
    return value.map(v => normalize(v, ''));
  }
  if (value && typeof value === 'object') {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value)) {
      out[k] = normalize(v, k);
    }
    return out;
  }
  return value;
}

const GOLDEN: GoldenFile = JSON.parse(readFileSync(join(import.meta.dir, '..', 'fixtures', 'golden.json'), 'utf8'));

describe('golden fixtures', () => {
  // The fixture file pins compression=none on the framed bytes for determinism;
  // a gzip-on codec would produce a different hex. The codec under test still
  // honors the encoded compression byte on decode regardless of the default.
  const codec = new Codec({ compression: Compression.None, encoding: Encoding.Msgpack });

  for (const fixture of GOLDEN.fixtures) {
    test(`decodes ${fixture.name}`, () => {
      const framed = hexToBytes(fixture.framed_hex);

      const header = parseFrameHeader(framed);
      expect(header.compression).toBe(Compression.None);
      expect(header.encoding).toBe(Encoding.Msgpack);
      expect(header.priority).toBe(fixture.priority);

      const decoded =
        fixture.direction === 'bridge_to_gateway'
          ? codec.decode<BridgeToGatewayMsg>(framed)
          : codec.decode<GatewayToBridgeMsg>(framed);

      expect(normalize(decoded, 'root')).toEqual(fixture.decoded_json);
    });

    test(`round-trips ${fixture.name}`, () => {
      const framed = hexToBytes(fixture.framed_hex);
      const decoded =
        fixture.direction === 'bridge_to_gateway'
          ? codec.decode<BridgeToGatewayMsg>(framed)
          : codec.decode<GatewayToBridgeMsg>(framed);

      const reEncoded = codec.encode(decoded, { priority: fixture.priority });
      const reHeader = parseFrameHeader(reEncoded);
      expect(reHeader.priority).toBe(fixture.priority);

      const redecoded =
        fixture.direction === 'bridge_to_gateway'
          ? codec.decode<BridgeToGatewayMsg>(reEncoded)
          : codec.decode<GatewayToBridgeMsg>(reEncoded);

      expect(normalize(redecoded, 'root')).toEqual(fixture.decoded_json);
    });
  }
});
