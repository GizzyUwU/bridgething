import {
  type BridgethingClient,
  type OtaError as OtaErrorMsg,
  type OtaPhase,
  type OtaProgress,
  type WebappInfo,
} from '@bridgething/client';
import { useEffect, useMemo, useState } from 'react';

type TileEntry = {
  info: WebappInfo;
  iconUrl: string | null;
};

type OtaSnapshot = {
  progress: OtaProgress | null;
  error: OtaErrorMsg | null;
};

export function Launcher({ client }: { client: BridgethingClient }) {
  const [tiles, setTiles] = useState<TileEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [activating, setActivating] = useState<TileEntry | null>(null);
  const [ota, setOta] = useState<OtaSnapshot>({ progress: null, error: null });

  useEffect(() => {
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
  }, [client]);

  useEffect(() => {
    const offProgress = client.system.onOtaProgress(progress => {
      setOta(prev => ({ progress, error: prev.error }));
    });
    const offError = client.system.onOtaError(err => {
      setOta(prev => ({ progress: prev.progress, error: err }));
    });
    return () => {
      offProgress();
      offError();
    };
  }, [client]);

  if (error) {
    return (
      <div className="flex h-full w-full items-center justify-center text-red-400">
        <div>{error}</div>
      </div>
    );
  }

  if (tiles === null) {
    return (
      <div className="flex h-full w-full items-center justify-center text-bt-soft-gray">
        <div className="text-sm">loading apps...</div>
      </div>
    );
  }

  if (tiles.length === 0) {
    return (
      <div className="flex h-full w-full items-center justify-center text-bt-soft-gray">
        <div className="text-sm">no apps installed.</div>
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
    <div className="relative flex h-full w-full flex-col bg-bt-charcoal px-4 py-3">
      <div className="bt-wordmark mb-2 text-xs font-medium uppercase tracking-[0.25em] text-bt-soft-gray">apps</div>
      <div className="flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-[43rem] flex-wrap content-start justify-center gap-3">
          {tiles.map(t => (
            <Tile key={t.info.id} entry={t} onActivate={onActivate} disabled={activating !== null} />
          ))}
        </div>
      </div>
      {activating && <ActivatingOverlay entry={activating} />}
      <OtaOverlay snapshot={ota} onDismiss={() => setOta({ progress: null, error: null })} />
    </div>
  );
}

const OTA_PHASE_LABELS: Record<OtaPhase, string> = {
  streaming: 'receiving update',
  verifying: 'verifying',
  writing: 'installing',
  confirming: 'confirming',
  reboot: 'rebooting',
};

function OtaOverlay({ snapshot, onDismiss }: { snapshot: OtaSnapshot; onDismiss: () => void }) {
  const { progress, error } = snapshot;
  if (!progress && !error) return null;
  return (
    <div className="absolute inset-0 z-40 flex flex-col items-center justify-center gap-4 bg-bt-charcoal/90 backdrop-blur-sm">
      {error ? (
        <>
          <div className="text-base font-medium text-red-300">update failed</div>
          <div className="max-w-[28rem] text-center text-xs text-bt-soft-gray">
            {error.code}: {error.msg}
          </div>
          <button
            type="button"
            onClick={onDismiss}
            className="mt-2 rounded-full bg-bt-soft-gray/20 px-4 py-1.5 text-xs text-bt-off-white active:scale-95">
            dismiss
          </button>
        </>
      ) : progress ? (
        <>
          <div className="text-xs uppercase tracking-[0.25em] text-bt-soft-gray">system update</div>
          <div className="text-lg font-medium text-bt-off-white">{OTA_PHASE_LABELS[progress.phase]}</div>
          <div className="h-2 w-64 overflow-hidden rounded-full bg-bt-soft-gray/20">
            <div
              className="h-full bg-bt-blue transition-[width] duration-200"
              style={{ width: `${Math.min(100, Math.max(0, progress.percent))}%` }}
            />
          </div>
          <div className="text-xs text-bt-soft-gray">{progress.percent}%</div>
        </>
      ) : null}
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
      className="flex w-32 flex-col items-center justify-center gap-2 rounded-2xl bg-black/30 p-3 transition-transform active:scale-95 disabled:opacity-60">
      <div
        className="flex h-20 w-20 items-center justify-center overflow-hidden rounded-xl"
        style={iconUrl ? undefined : { background: fallback.background, color: fallback.foreground }}>
        {iconUrl ? (
          <img src={iconUrl} alt="" className="h-full w-full object-contain" draggable={false} />
        ) : (
          <span className="bt-wordmark text-2xl font-medium">{fallback.letter}</span>
        )}
      </div>
      <span className="line-clamp-1 w-full text-center text-sm font-medium text-bt-off-white">{info.name}</span>
    </button>
  );
}

function ActivatingOverlay({ entry }: { entry: TileEntry }) {
  const { info, iconUrl } = entry;
  const fallback = useMemo(() => fallbackStyle(info), [info]);
  return (
    <div className="absolute inset-0 z-50 flex flex-col items-center justify-center gap-6 bg-bt-charcoal">
      <div
        className="flex h-28 w-28 items-center justify-center overflow-hidden rounded-2xl"
        style={iconUrl ? undefined : { background: fallback.background, color: fallback.foreground }}>
        {iconUrl ? (
          <img src={iconUrl} alt="" className="h-full w-full object-contain" draggable={false} />
        ) : (
          <span className="bt-wordmark text-4xl font-medium">{fallback.letter}</span>
        )}
      </div>
      <div className="text-lg font-medium text-bt-off-white">{info.name}</div>
      <div className="h-8 w-8 animate-spin rounded-full border-2 border-bt-soft-gray/30 border-t-bt-blue" />
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
