import { gzipSync as deflate, gunzipSync as inflate } from 'fflate';

export const constants = {
  Z_NO_COMPRESSION: 0,
  Z_BEST_SPEED: 1,
  Z_BEST_COMPRESSION: 9,
  Z_DEFAULT_COMPRESSION: -1,
} as const;

function toBytes(data: Uint8Array | string): Uint8Array {
  return typeof data === 'string' ? new TextEncoder().encode(data) : new Uint8Array(data);
}

export function gunzipSync(data: Uint8Array | string): Uint8Array {
  return inflate(toBytes(data));
}

export function gzipSync(data: Uint8Array | string, options?: { level?: number }): Uint8Array {
  const level = options?.level;
  return deflate(toBytes(data), {
    level: level === undefined || level < 0 || level > 9 ? 6 : (level as 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9),
  });
}

export default { constants, gunzipSync, gzipSync };
