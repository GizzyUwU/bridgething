import {
  type BridgethingClient,
  type OtaError as OtaErrorMsg,
  type OtaPhase,
  type OtaProgress,
  type WebappInfo,
} from '@bridgething/client';
import { useEffect, useMemo, useState } from 'react';

import { Settings } from './settings';

type TileEntry = {
  info: WebappInfo;
  iconUrl: string | null;
};

// `attempt` bumps whenever a fresh update starts or fails, so dismissing one run cannot hide the next
type OtaSnapshot = {
  attempt: number;
  progress: OtaProgress | null;
  error: OtaErrorMsg | null;
  dismissed: number | null;
};

const OTA_IDLE: OtaSnapshot = { attempt: 0, progress: null, error: null, dismissed: null };

function otaVisible(snapshot: OtaSnapshot): boolean {
  return (snapshot.progress !== null || snapshot.error !== null) && snapshot.dismissed !== snapshot.attempt;
}

export function Launcher({ client }: { client: BridgethingClient }) {
  const [tiles, setTiles] = useState<TileEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [activating, setActivating] = useState<TileEntry | null>(null);
  const [ota, setOta] = useState<OtaSnapshot>(OTA_IDLE);
  const [view, setView] = useState<'apps' | 'settings'>('apps');

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
            if (!info.iconHash) return { info, iconUrl: null };
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
      setOta(prev =>
        prev.progress && !prev.error && !isNewRun(prev.progress, progress)
          ? { ...prev, progress }
          : { attempt: prev.attempt + 1, progress, error: null, dismissed: prev.dismissed },
      );
    });
    const offError = client.system.onOtaError(error => {
      setOta(prev => ({ attempt: prev.attempt + 1, progress: null, error, dismissed: prev.dismissed }));
    });
    return () => {
      offProgress();
      offError();
    };
  }, [client]);

  if (view === 'settings') {
    return <Settings client={client} onClose={() => setView('apps')} />;
  }

  const onActivate = async (entry: TileEntry) => {
    if (activating) return;
    setActivating(entry);
    const r = await client.webapp.activate({ id: entry.info.id });
    if (!r.ok) setActivating(null);
  };

  const body = error ? (
    <div className="flex flex-1 items-center justify-center text-red-400">
      <div>{error}</div>
    </div>
  ) : tiles === null ? (
    <div className="flex flex-1 items-center justify-center text-bt-soft-gray">
      <div className="text-sm">loading apps...</div>
    </div>
  ) : tiles.length === 0 ? (
    <div className="flex flex-1 items-center justify-center text-bt-soft-gray">
      <div className="text-sm">no apps installed.</div>
    </div>
  ) : (
    <div className="flex-1 overflow-y-auto">
      <div className="mx-auto flex max-w-[43rem] flex-wrap content-start justify-center gap-3">
        {tiles.map(t => (
          <Tile key={t.info.id} entry={t} onActivate={onActivate} disabled={activating !== null} />
        ))}
      </div>
    </div>
  );

  return (
    <Shell
      onSettings={() => setView('settings')}
      status={
        otaVisible(ota) ? null : (
          <OtaChip snapshot={ota} onResume={() => setOta(prev => ({ ...prev, dismissed: null }))} />
        )
      }>
      {body}
      {activating && <ActivatingOverlay entry={activating} />}
      {otaVisible(ota) && (
        <OtaOverlay snapshot={ota} onDismiss={() => setOta(prev => ({ ...prev, dismissed: prev.attempt }))} />
      )}
    </Shell>
  );
}

function Shell({
  onSettings,
  status,
  children,
}: {
  onSettings: () => void;
  status?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="relative flex h-full w-full flex-col bg-bt-charcoal px-4 py-3">
      <div className="mb-2 flex items-center justify-between">
        <div className="bt-wordmark text-xs font-medium uppercase tracking-[0.25em] text-bt-soft-gray">apps</div>
        <div className="flex items-center gap-2">
          {status}
          <button
            type="button"
            onClick={onSettings}
            aria-label="settings"
            className="flex h-9 w-9 items-center justify-center rounded-full bg-black/30 text-bt-soft-gray transition active:scale-90">
            <GearIcon />
          </button>
        </div>
      </div>
      {children}
    </div>
  );
}

function OtaChip({ snapshot, onResume }: { snapshot: OtaSnapshot; onResume: () => void }) {
  const { progress, error } = snapshot;
  if (!progress && !error) return null;
  return (
    <button
      type="button"
      onClick={onResume}
      className="flex items-center gap-2 rounded-full bg-black/30 py-1.5 pr-3.5 pl-3 text-xs transition active:scale-95">
      <span className={`h-1.5 w-1.5 rounded-full ${error ? 'bg-red-400' : 'animate-pulse bg-bt-blue'}`} />
      <span className={error ? 'text-red-300' : 'text-bt-soft-gray'}>
        {error ? 'update failed' : progress ? OTA_PHASES[progress.phase].label : null}
      </span>
    </button>
  );
}

function GearIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <circle cx="12" cy="12" r="3" />
      <path
        d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

const OTA_PHASES: Record<OtaPhase, { label: string; rank: number }> = {
  streaming: { label: 'receiving update', rank: 0 },
  verifying: { label: 'verifying', rank: 1 },
  writing: { label: 'installing', rank: 2 },
  confirming: { label: 'confirming', rank: 3 },
  reboot: { label: 'rebooting', rank: 4 },
};

// phases only ever advance within one update, so a regression means a second update started
function isNewRun(prev: OtaProgress, next: OtaProgress): boolean {
  return next.step < prev.step || OTA_PHASES[next.phase].rank < OTA_PHASES[prev.phase].rank;
}

function OtaOverlay({ snapshot, onDismiss }: { snapshot: OtaSnapshot; onDismiss: () => void }) {
  const { progress, error } = snapshot;
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
          <div className="text-lg font-medium text-bt-off-white">{OTA_PHASES[progress.phase].label}</div>
          <div className="h-2 w-64 overflow-hidden rounded-full bg-bt-soft-gray/20">
            <div
              className="h-full bg-bt-blue transition-[width] duration-200"
              style={{ width: `${Math.min(100, Math.max(0, progress.percent))}%` }}
            />
          </div>
          <div className="text-xs text-bt-soft-gray">{progress.percent}%</div>
          <button
            type="button"
            onClick={onDismiss}
            className="mt-2 rounded-full bg-bt-soft-gray/20 px-4 py-1.5 text-xs text-bt-off-white active:scale-95">
            hide
          </button>
          <div className="text-[0.6875rem] text-bt-soft-gray/70">installing continues in the background</div>
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
