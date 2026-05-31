import { BridgethingClient } from '@bridgething/client';
import { useEffect, useMemo, useState } from 'react';

const wsUrl =
  import.meta.env.VITE_BRIDGETHING_URL ??
  (typeof window !== 'undefined' ? `ws://${window.location.host}/` : 'ws://127.0.0.1:8891/');

type Bookmark = { label: string; url: string };

export default function App() {
  const client = useMemo(() => new BridgethingClient({ url: wsUrl }), []);
  const [bookmarks, setBookmarks] = useState<Bookmark[]>([]);
  const [draft, setDraft] = useState('');

  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      const cfg = await client.config.get({ key: 'bookmarks' });
      const raw = cfg.ok ? cfg.response.value : null;
      if (!cancelled) setBookmarks(parseBookmarks(raw));
    };
    load();
    const off = client.config.onChanged(() => load());
    return () => {
      cancelled = true;
      off();
    };
  }, [client]);

  const go = (url: string) => {
    const normalized = normalizeUrl(url);
    if (normalized) window.location.href = normalized;
  };

  return (
    <div className="flex h-full w-full flex-col bg-bt-charcoal px-10 py-8 text-bt-off-white">
      <div className="bt-wordmark mb-5 text-2xl font-semibold">Browser</div>

      <form
        className="mb-7 flex gap-3"
        onSubmit={e => {
          e.preventDefault();
          if (draft.trim()) go(draft);
        }}>
        <input
          type="url"
          inputMode="url"
          autoCapitalize="none"
          autoCorrect="off"
          spellCheck={false}
          value={draft}
          onChange={e => setDraft(e.target.value)}
          placeholder="enter a url"
          className="min-w-0 flex-1 rounded-xl bg-black/30 px-4 py-3 text-base text-bt-off-white placeholder:text-bt-soft-gray focus:outline-none focus:ring-2 focus:ring-bt-blue"
        />
        <button
          type="submit"
          disabled={!draft.trim()}
          className="rounded-xl bg-bt-blue px-6 py-3 text-base font-medium text-bt-charcoal disabled:opacity-40">
          go
        </button>
      </form>

      {bookmarks.length === 0 ? (
        <div className="flex flex-1 items-center justify-center">
          <div className="max-w-[32rem] text-center text-sm text-bt-soft-gray">
            add bookmarks in the companion app, or type a url above. sites load through the connected phone.
          </div>
        </div>
      ) : (
        <div className="grid flex-1 grid-cols-3 content-start gap-3 overflow-y-auto">
          {bookmarks.map((b, i) => (
            <button
              key={i}
              onClick={() => go(b.url)}
              className="flex flex-col gap-1 rounded-2xl bg-black/30 px-4 py-4 text-left active:bg-black/50">
              <span className="truncate text-base font-medium text-bt-off-white">{b.label}</span>
              <span className="truncate text-xs text-bt-soft-gray">{hostOf(b.url)}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function parseBookmarks(raw: string | null): Bookmark[] {
  if (!raw) return [];
  return raw
    .split(/[\n,]/)
    .map(s => s.trim())
    .filter(Boolean)
    .map(entry => {
      const bar = entry.indexOf('|');
      if (bar >= 0) {
        const label = entry.slice(0, bar).trim();
        const url = entry.slice(bar + 1).trim();
        return { label: label || hostOf(url), url };
      }
      return { label: hostOf(entry), url: entry };
    })
    .filter(b => b.url.length > 0);
}

function normalizeUrl(input: string): string | null {
  const trimmed = input.trim();
  if (!trimmed) return null;
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(trimmed)) return trimmed;
  return `https://${trimmed}`;
}

function hostOf(url: string): string {
  const withScheme = /^[a-z][a-z0-9+.-]*:\/\//i.test(url) ? url : `https://${url}`;
  try {
    return new URL(withScheme).host || url;
  } catch {
    return url;
  }
}
