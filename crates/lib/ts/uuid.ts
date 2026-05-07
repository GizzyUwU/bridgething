import { parse, stringify, v7 } from 'uuid';

export function uuidToString(bytes: Uint8Array): string {
  return stringify(bytes);
}

export function uuidFromString(value: string): Uint8Array {
  return parse(value);
}

/** Time-ordered UUIDv7 as a hyphenated string; matches the daemon's `Uuid::now_v7()`. */
export function newUuid(): string {
  return v7();
}

export function bytesEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a[i] ^ b[i];
  return diff === 0;
}
