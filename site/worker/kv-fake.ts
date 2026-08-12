import type { KvLike } from './store.ts';

export type FakeKv = KvLike & {
  snapshot(): Record<string, string>;
  counts: { get: number; put: number; list: number };
  resetCounts(): void;
};

export function fakeKv(seed: Record<string, string> = {}): FakeKv {
  const data = new Map<string, string>(Object.entries(seed));
  const counts = { get: 0, put: 0, list: 0 };

  return {
    counts,
    async get(key) {
      counts.get += 1;
      return data.get(key) ?? null;
    },
    async put(key, value) {
      counts.put += 1;
      data.set(key, value);
    },
    async delete(key) {
      data.delete(key);
    },
    async list({ prefix = '' } = {}) {
      counts.list += 1;
      const keys = [...data.keys()]
        .filter(name => name.startsWith(prefix))
        .sort()
        .map(name => ({ name }));
      return { keys, list_complete: true };
    },
    snapshot() {
      return Object.fromEntries(data);
    },
    resetCounts() {
      counts.get = 0;
      counts.put = 0;
      counts.list = 0;
    },
  };
}
