const HEX = '0123456789abcdef';

const toHex = (b: number) => HEX[(b >> 4) & 0xf] + HEX[b & 0xf];

const HEX_LOOKUP: Record<string, number> = (() => {
  const m: Record<string, number> = {};
  for (let i = 0; i < 16; i++) m[HEX[i]] = i;
  for (let i = 10; i < 16; i++) m[HEX[i].toUpperCase()] = i;
  return m;
})();

/** 16 bytes → canonical 8-4-4-4-12 lowercase string. */
export function uuidToString(bytes: Uint8Array): string {
  if (bytes.length !== 16) throw new Error(`uuid bytes must be length 16 (got ${bytes.length})`);
  let s = '';
  for (let i = 0; i < 16; i++) {
    s += toHex(bytes[i]);
    if (i === 3 || i === 5 || i === 7 || i === 9) s += '-';
  }
  return s;
}

/** Canonical UUID string → 16 bytes. Hyphens optional. */
export function uuidFromString(value: string): Uint8Array {
  const stripped = value.replace(/-/g, '');
  if (stripped.length !== 32) throw new Error(`uuid string must decode to 16 bytes (got ${value})`);
  const out = new Uint8Array(16);
  for (let i = 0; i < 16; i++) {
    const hi = HEX_LOOKUP[stripped[i * 2]];
    const lo = HEX_LOOKUP[stripped[i * 2 + 1]];
    if (hi === undefined || lo === undefined) throw new Error(`invalid uuid char in ${value}`);
    out[i] = (hi << 4) | lo;
  }
  return out;
}

/** Cryptographically-strong UUID v4 as 16 bytes (browser/RN/Node 19+). */
export function newUuidBytes(): Uint8Array {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  return bytes;
}

/** Constant-time compare of two byte arrays. */
export function bytesEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a[i] ^ b[i];
  return diff === 0;
}
