export function bytes(count: number): string {
  if (count < 1024) return `${count} B`;
  if (count < 1024 * 1024) return `${Math.round(count / 1024)} KB`;
  if (count < 1024 * 1024 * 1024) return `${(count / (1024 * 1024)).toFixed(1)} MB`;
  return `${(count / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

export function rate(perSecond: number | null): string | null {
  return perSecond === null || perSecond <= 0 ? null : `${bytes(perSecond)}/s`;
}

export function clock(unixMs: number): string {
  const at = new Date(unixMs);
  const pad = (value: number, width = 2) => String(value).padStart(width, '0');
  return `${pad(at.getHours())}:${pad(at.getMinutes())}:${pad(at.getSeconds())}.${pad(at.getMilliseconds(), 3)}`;
}

export function day(raw: string): string {
  const at = Date.parse(raw);
  return Number.isNaN(at) ? raw : new Date(at).toLocaleDateString();
}

export function since(raw: string | null): string {
  if (!raw) return 'never';
  const at = Date.parse(raw);
  if (Number.isNaN(at)) return raw;
  const seconds = Math.max(0, (Date.now() - at) / 1000);
  if (seconds < 45) return 'just now';
  if (seconds < 3600) return `${Math.round(seconds / 60)}m ago`;
  if (seconds < 86_400) return `${Math.round(seconds / 3600)}h ago`;
  return `${Math.round(seconds / 86_400)}d ago`;
}

export function peerHost(id: string): string {
  try {
    return new URL(id).host;
  } catch {
    return id;
  }
}

export function basename(path: string): string {
  const cut = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  return cut === -1 ? path : path.slice(cut + 1);
}
