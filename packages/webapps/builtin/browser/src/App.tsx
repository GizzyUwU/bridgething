import { BridgethingClient } from '@bridgething/client';
import { daemonUrl } from '@bridgething/webapp-shared/daemon';
import { useEffect, useMemo, useState } from 'react';

type Bookmark = { label: string; url: string };

export default function App() {
  const client = useMemo(() => new BridgethingClient({ url: daemonUrl() }), []);
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
    if (normalized) client.store.put({ key: '@cdp/navigate', value: normalized });
  };

  return (
    <div className="flex h-full w-full flex-col bg-bg px-10 py-7 text-off-white">
      <div className="mb-5 border-b border-rule pb-3 font-mono text-eyebrow tracking-[0.25em] text-dim uppercase">
        browser
      </div>

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
          className="min-w-0 flex-1 border border-edge bg-screen px-4 py-3 font-mono text-row-lg text-off-white outline-none placeholder:text-dim focus:border-accent"
        />
        <button
          type="submit"
          disabled={!draft.trim()}
          className="border border-accent bg-accent px-8 py-3 font-mono text-row-lg text-screen disabled:border-rule disabled:bg-transparent disabled:text-dim">
          go
        </button>
      </form>

      {bookmarks.length === 0 ? (
        <div className="flex flex-1 items-center justify-center">
          <div className="max-w-lg text-center text-row text-near">
            add bookmarks in the companion app, or type a url above. sites load through the connected phone.
          </div>
        </div>
      ) : (
        <div className="grid flex-1 grid-cols-3 content-start gap-3 overflow-y-auto">
          {bookmarks.map((b, i) => (
            <button
              key={i}
              onClick={() => go(b.url)}
              className="flex flex-col gap-1 border border-rule bg-screen px-4 py-4 text-left active:border-edge active:bg-neutral-soft">
              <span className="truncate text-row-lg font-medium text-off-white">{b.label}</span>
              <span className="truncate font-mono text-hint text-dim">{hostOf(b.url)}</span>
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
