const UNREADABLE = 'the reason was lost';

export function describeError(reason: unknown): string {
  if (reason instanceof Error) return reason.message;
  if (typeof reason === 'string') return reason;
  if (typeof reason === 'number' || typeof reason === 'boolean') return String(reason);
  if (reason === null || reason === undefined) return UNREADABLE;

  if (typeof reason === 'object') {
    const held = reason as Record<string, unknown>;
    const detail = held.reason ?? held.message ?? held.error;
    if (typeof detail === 'string' && detail.trim()) return detail.trim();
    if (typeof held.kind === 'string' && held.kind.trim()) return spaced(held.kind);
    if (detail !== undefined && detail !== null) return describeError(detail);
  }

  try {
    const encoded = JSON.stringify(reason);
    if (encoded && encoded !== '{}' && encoded !== 'null') return encoded;
  } catch {
    // cyclic rejections fall through to the fallback
  }
  return typeof reason === 'object' ? UNREADABLE : String(reason);
}

function spaced(kind: string): string {
  return kind
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .replace(/[_-]+/g, ' ')
    .toLowerCase();
}
