import { BridgethingClient, type WebappInfo } from '@bridgething/client';
import { useEffect, useMemo, useState } from 'react';

const client = new BridgethingClient();

type TileEntry = {
  info: WebappInfo;
  iconUrl: string | null;
};

export default function App() {
  const [tiles, setTiles] = useState<TileEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [activating, setActivating] = useState<TileEntry | null>(null);

  useEffect(() => {
    client.connect();
    let cancelled = false;
    let revoke: string[] = [];

    (async () => {
      try {
        const [listResult, currentResult] = await Promise.all([client.webapp.list(), client.webapp.current()]);
        if (!listResult.ok) {
          setError('failed to list webapps');
          return;
        }
        const selfId = currentResult.ok ? currentResult.response.id : null;
        const visible = listResult.response.webapps.filter(w => w.id !== selfId);
        const entries: TileEntry[] = await Promise.all(
          visible.map(async info => {
            if (!info.iconAvailable) return { info, iconUrl: null };
            const iconResult = await client.webapp.icon({ id: info.id });
            if (!iconResult.ok) return { info, iconUrl: null };
            const bytes = new Uint8Array(iconResult.response.bytes as unknown as number[]);
            const blob = new Blob([bytes], {
              type: iconResult.response.mime ?? 'application/octet-stream',
            });
            const url = URL.createObjectURL(blob);
            revoke.push(url);
            return { info, iconUrl: url };
          }),
        );
        if (!cancelled) setTiles(entries);
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      }
    })();

    return () => {
      cancelled = true;
      for (const url of revoke) URL.revokeObjectURL(url);
    };
  }, []);

  if (error) {
    return (
      <div className="flex h-full w-full items-center justify-center text-red-400">
        <div>{error}</div>
      </div>
    );
  }

  if (tiles === null) {
    return (
      <div className="flex h-full w-full items-center justify-center text-neutral-400">
        <div className="text-sm">Loading apps...</div>
      </div>
    );
  }

  if (tiles.length === 0) {
    return (
      <div className="flex h-full w-full items-center justify-center text-neutral-500">
        <div className="text-sm">No apps installed.</div>
      </div>
    );
  }

  const onActivate = async (entry: TileEntry) => {
    if (activating) return;
    setActivating(entry);
    const r = await client.webapp.activate({ id: entry.info.id });
    if (!r.ok) setActivating(null);
  };

  return (
    <div className="relative flex h-full w-full flex-col px-4 py-3">
      <div className="mb-2 text-xs uppercase tracking-widest text-neutral-500">Apps</div>
      <div className="flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-[43rem] flex-wrap content-start justify-center gap-3">
          {tiles.map(t => (
            <Tile key={t.info.id} entry={t} onActivate={onActivate} disabled={activating !== null} />
          ))}
        </div>
      </div>
      {activating && <ActivatingOverlay entry={activating} />}
    </div>
  );
}

function Tile({
  entry,
  onActivate,
  disabled,
}: {
  entry: TileEntry;
  onActivate: (entry: TileEntry) => void;
  disabled: boolean;
}) {
  const { info, iconUrl } = entry;
  const fallback = useMemo(() => fallbackStyle(info), [info]);

  return (
    <button
      type="button"
      onClick={() => onActivate(entry)}
      disabled={disabled}
      className="flex w-32 flex-col items-center justify-center gap-2 rounded-2xl bg-neutral-900 p-3 transition-transform active:scale-95 disabled:opacity-60">
      <div
        className="flex h-20 w-20 items-center justify-center overflow-hidden rounded-xl"
        style={iconUrl ? undefined : { background: fallback.background, color: fallback.foreground }}>
        {iconUrl ? (
          <img src={iconUrl} alt="" className="h-full w-full object-contain" draggable={false} />
        ) : (
          <span className="text-2xl font-semibold">{fallback.letter}</span>
        )}
      </div>
      <span className="line-clamp-1 w-full text-center text-sm font-medium text-neutral-100">{info.name}</span>
    </button>
  );
}

function ActivatingOverlay({ entry }: { entry: TileEntry }) {
  const { info, iconUrl } = entry;
  const fallback = useMemo(() => fallbackStyle(info), [info]);
  return (
    <div className="absolute inset-0 z-50 flex flex-col items-center justify-center gap-6 bg-black">
      <div
        className="flex h-28 w-28 items-center justify-center overflow-hidden rounded-2xl"
        style={iconUrl ? undefined : { background: fallback.background, color: fallback.foreground }}>
        {iconUrl ? (
          <img src={iconUrl} alt="" className="h-full w-full object-contain" draggable={false} />
        ) : (
          <span className="text-4xl font-semibold">{fallback.letter}</span>
        )}
      </div>
      <div className="text-lg font-medium text-neutral-100">{info.name}</div>
      <div className="h-8 w-8 animate-spin rounded-full border-2 border-neutral-700 border-t-neutral-200" />
    </div>
  );
}

function fallbackStyle(info: WebappInfo): { letter: string; background: string; foreground: string } {
  const letter = (info.name.trim().charAt(0) || '?').toUpperCase();
  const id = String(info.id).replace(/-/g, '');
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) >>> 0;
  const hue = h % 360;
  const background = `hsl(${hue}deg 55% 32%)`;
  const foreground = `hsl(${hue}deg 30% 92%)`;
  return { letter, background, foreground };
}
