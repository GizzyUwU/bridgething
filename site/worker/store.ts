import { KEY_PREFIX, keyFor, type SourceRecord } from './directory.ts';

export type KvLike = {
  get(key: string): Promise<string | null>;
  put(key: string, value: string, options?: { expirationTtl?: number }): Promise<unknown>;
  delete(key: string): Promise<unknown>;
  list(options: {
    prefix?: string;
    cursor?: string;
  }): Promise<{ keys: { name: string }[]; list_complete: boolean; cursor?: string }>;
};

export async function readRecord<T>(kv: KvLike, key: string): Promise<T | null> {
  const raw = await kv.get(key);
  if (raw === null) return null;
  try {
    return JSON.parse(raw) as T;
  } catch {
    return null;
  }
}

export async function readList<T>(kv: KvLike, key: string): Promise<T[] | null> {
  const parsed = await readRecord<unknown>(kv, key);
  return Array.isArray(parsed) ? (parsed as T[]) : null;
}

export async function putList<T>(kv: KvLike, key: string, records: T[]): Promise<void> {
  await kv.put(key, JSON.stringify(records));
}

export async function walkRecords<T>(kv: KvLike, prefix: string): Promise<T[]> {
  const out: T[] = [];
  let cursor: string | undefined;

  do {
    const page = await kv.list({ prefix, cursor });
    for (const { name } of page.keys) {
      const record = await readRecord<T>(kv, name);
      if (record !== null) out.push(record);
    }
    cursor = page.list_complete ? undefined : page.cursor;
  } while (cursor);

  return out;
}

const SOURCE_SNAPSHOT_KEY = 'directory:snapshot';

export async function readSource(kv: KvLike, url: string): Promise<SourceRecord | null> {
  return readRecord<SourceRecord>(kv, keyFor(url));
}

export async function writeSource(kv: KvLike, record: SourceRecord): Promise<void> {
  await kv.put(keyFor(record.url), JSON.stringify(record));
  // kv has no cas: editing the snapshot here drops a concurrent write, so invalidate and let listSources rebuild it.
  await kv.delete(SOURCE_SNAPSHOT_KEY);
}

export async function rebuildSources(kv: KvLike): Promise<SourceRecord[]> {
  const records = await walkRecords<SourceRecord>(kv, KEY_PREFIX);
  await putList(kv, SOURCE_SNAPSHOT_KEY, records);
  return records;
}

export async function listSources(kv: KvLike): Promise<SourceRecord[]> {
  return (await readList<SourceRecord>(kv, SOURCE_SNAPSHOT_KEY)) ?? (await rebuildSources(kv));
}

const RATE_LIMIT_PREFIX = 'rl:';

export async function takeRateLimitToken(
  kv: KvLike,
  client: string,
  limit: number,
  windowSeconds: number,
): Promise<boolean> {
  const key = `${RATE_LIMIT_PREFIX}${client}`;
  const current = Number.parseInt((await kv.get(key)) ?? '0', 10);
  const used = Number.isNaN(current) ? 0 : current;
  if (used >= limit) return false;
  await kv.put(key, String(used + 1), { expirationTtl: windowSeconds });
  return true;
}
