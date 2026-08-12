import { parse, stringify, v7 } from 'uuid';

import { UUID_FIELD_NAMES } from './uuid-fields.generated.js';

export function uuidToString(bytes: Uint8Array): string {
  return stringify(bytes);
}

export function uuidFromString(value: string): Uint8Array {
  return parse(value);
}

export function newUuid(): string {
  return v7();
}

export function bytesEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a[i] ^ b[i];
  return diff === 0;
}

const UUID_REGEX = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

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
