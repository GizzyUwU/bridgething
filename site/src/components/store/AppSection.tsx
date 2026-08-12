import { useMemo, useState } from 'preact/hooks';
import type { CatalogAppListing } from '@bridgething/catalog';
import type { StoreSource } from '../../lib/store-sources';
import { AppCard } from './AppCard';

const ALL = '';

function chip(active: boolean): string {
  return active ? 'btn btn-primary text-sm' : 'btn text-sm';
}

export function AppSection({
  title,
  note,
  status,
  empty,
  listings,
  sources,
}: {
  title: string;
  note?: string;
  status: string;
  empty: string;
  listings: CatalogAppListing[];
  sources: Map<string, StoreSource>;
}) {
  const [filter, setFilter] = useState(ALL);

  const contributing = useMemo(() => {
    const present = new Set(listings.map(listing => listing.sourceUrl));
    return [...sources.values()].filter(source => present.has(source.url));
  }, [listings, sources]);

  const shown = filter === ALL ? listings : listings.filter(listing => listing.sourceUrl === filter);

  return (
    <section class="mb-16">
      <header class="mb-4 flex flex-wrap items-baseline justify-between gap-3 border-b border-white/20 pb-2">
        <h2 class="m-0">{title}</h2>
        <p class="m-0 font-mono text-sm text-white/40">{status}</p>
      </header>

      {note ? <p class="mb-6 max-w-[70ch] text-sm text-white/55">{note}</p> : null}

      {contributing.length > 1 ? (
        <div class="mb-6 flex flex-wrap gap-2">
          <button type="button" class={chip(filter === ALL)} onClick={() => setFilter(ALL)}>
            everything
          </button>
          {contributing.map(source => (
            <button
              key={source.url}
              type="button"
              class={chip(filter === source.url)}
              onClick={() => setFilter(source.url)}>
              {source.name}
            </button>
          ))}
        </div>
      ) : null}

      {shown.length === 0 ? (
        <div class="border border-dashed border-white/25 p-16 text-center">
          <p class="m-0 text-white/60">{empty}</p>
        </div>
      ) : (
        <div class="grid grid-cols-1 gap-6 md:grid-cols-2 lg:grid-cols-3">
          {shown.map(listing => (
            <AppCard key={listing.app.id} listing={listing} source={sources.get(listing.sourceUrl) ?? null} />
          ))}
        </div>
      )}
    </section>
  );
}
