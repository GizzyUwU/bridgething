export type BoundedCache<T> = {
  get(key: string): T | undefined;
  set(key: string, value: T): void;
};

export function boundedCache<T>(limit: number): BoundedCache<T> {
  const entries = new Map<string, T>();
  return {
    get(key) {
      const hit = entries.get(key);
      if (hit === undefined) return undefined;
      entries.delete(key);
      entries.set(key, hit);
      return hit;
    },
    set(key, value) {
      entries.delete(key);
      entries.set(key, value);
      for (const oldest of entries.keys()) {
        if (entries.size <= limit) break;
        entries.delete(oldest);
      }
    },
  };
}
