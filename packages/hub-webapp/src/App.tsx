import { BridgethingClient, type WebappInfo } from '@bridgething/client';
import { useEffect, useMemo, useState } from 'react';

const HUB_UUID = '019693c0-5c6a-71f0-a89d-7e2a4d9c0a01';

const client = new BridgethingClient();

type TileEntry = {
  info: WebappInfo;
  iconUrl: string | null;
};

export default function App() {
  const [tiles, setTiles] = useState<TileEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    client.connect();
    let cancelled = false;
    let revoke: string[] = [];

    (async () => {
      try {
        const result = await client.webapp.list();
        if (!result.ok) {
          setError('failed to list webapps');
          return;
        }
        const visible = result.response.webapps.filter(w => w.id !== HUB_UUID);
        const entries: TileEntry[] = await Promise.all(
          visible.map(async info => {
            if (!info.iconAvailable) return { info, iconUrl: null };
            const iconResult = await client.webapp.icon({ id: info.id });
            if (!iconResult.ok) return { info, iconUrl: null };
            const blob = new Blob([iconResult.response.bytes], {
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

  return (
    <div className="flex h-full w-full flex-col px-6 py-5">
      <div className="mb-4 text-xs uppercase tracking-widest text-neutral-500">Apps</div>
      <div className="grid flex-1 grid-cols-4 gap-4 overflow-y-auto">
        {tiles.map(t => (
          <Tile key={t.info.id} entry={t} />
        ))}
      </div>
    </div>
  );
}

function Tile({ entry }: { entry: TileEntry }) {
  const { info, iconUrl } = entry;
  const fallback = useMemo(() => fallbackStyle(info), [info]);

  const onActivate = async () => {
    await client.webapp.activate({ id: info.id });
  };

  return (
    <button
      type="button"
      onClick={onActivate}
      className="flex flex-col items-center justify-center gap-2 rounded-2xl bg-neutral-900 p-4 active:scale-95 transition-transform">
      <div
        className="flex h-24 w-24 items-center justify-center overflow-hidden rounded-xl"
        style={iconUrl ? undefined : { background: fallback.background, color: fallback.foreground }}>
        {iconUrl ? (
          <img src={iconUrl} alt="" className="h-full w-full object-contain" draggable={false} />
        ) : (
          <span className="text-3xl font-semibold">{fallback.letter}</span>
        )}
      </div>
      <span className="line-clamp-1 text-sm font-medium text-neutral-100">{info.name}</span>
    </button>
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
