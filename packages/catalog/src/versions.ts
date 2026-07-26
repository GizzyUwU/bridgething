export function releasedAtInstant(raw: string): number | null {
  const parsed = Date.parse(raw);
  return Number.isNaN(parsed) ? null : parsed;
}

export function sortNewestFirst<T extends { released_at: string }>(items: T[]): T[] {
  return items
    .map((item, index) => ({ item, index, at: releasedAtInstant(item.released_at) }))
    .sort((a, b) => {
      if (a.at === null && b.at === null) return a.index - b.index;
      if (a.at === null) return 1;
      if (b.at === null) return -1;
      return a.at === b.at ? a.index - b.index : b.at - a.at;
    })
    .map(entry => entry.item);
}
