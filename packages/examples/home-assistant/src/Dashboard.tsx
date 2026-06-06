import type { Tile } from './App';
import { controlKind, domainIcon, friendlyName, isActive, num } from './domains';
import type { HaState, HaStatus } from './ha';

type Props = {
  tiles: Tile[];
  status: HaStatus;
  toast: string | null;
  onActivate: (s: HaState) => void;
  onSetTemp: (entityId: string, target: number) => void;
  onOpenPicker: () => void;
};

export default function Dashboard({ tiles, status, toast, onActivate, onSetTemp, onOpenPicker }: Props) {
  const live = tiles.some(t => t.state);
  if (!live && status.kind === 'error') return <FullError message={status.message} />;

  return (
    <div className="relative flex h-full w-full flex-col bg-bt-charcoal text-bt-off-white">
      <header className="flex items-center justify-between px-6 pt-4 pb-2">
        <div className="flex items-baseline gap-3">
          <span className="bt-wordmark text-xl font-semibold">Home Assistant</span>
          {status.kind !== 'ready' && <span className="text-xs text-bt-soft-gray">{statusLabel(status)}</span>}
        </div>
        <button
          onClick={onOpenPicker}
          className="rounded-full bg-black/30 px-4 py-1.5 text-xs text-bt-soft-gray active:bg-black/50">
          edit tiles
        </button>
      </header>

      <div className="grid flex-1 grid-flow-col grid-rows-3 auto-cols-[176px] gap-3 overflow-x-auto px-6 pb-5">
        {tiles.map(t => (
          <TileView key={t.entityId} tile={t} onActivate={onActivate} onSetTemp={onSetTemp} />
        ))}
      </div>

      {toast && (
        <div className="pointer-events-none absolute inset-x-0 bottom-4 flex justify-center">
          <div className="rounded-full bg-black/70 px-5 py-2 text-xs text-bt-off-white">{toast}</div>
        </div>
      )}
    </div>
  );
}

function TileView({
  tile,
  onActivate,
  onSetTemp,
}: {
  tile: Tile;
  onActivate: Props['onActivate'];
  onSetTemp: Props['onSetTemp'];
}) {
  const { entityId, state } = tile;
  if (!state || state.state === 'unavailable') {
    return (
      <div className="flex flex-col justify-between rounded-2xl bg-black/20 p-4 opacity-50">
        <div className="text-2xl">{domainIcon(entityId)}</div>
        <div className="truncate text-sm">{entityId}</div>
        <div className="text-xs text-bt-soft-gray">unavailable</div>
      </div>
    );
  }

  const kind = controlKind(entityId);
  if (kind === 'climate') return <ClimateTile tile={tile} state={state} onSetTemp={onSetTemp} />;
  if (kind === 'readonly') return <ReadonlyTile state={state} />;
  return <ActionTile state={state} kind={kind} onActivate={onActivate} />;
}

function ActionTile({
  state,
  kind,
  onActivate,
}: {
  state: HaState;
  kind: 'toggle' | 'lock' | 'momentary';
  onActivate: Props['onActivate'];
}) {
  const active = isActive(state);
  const accent = kind !== 'momentary' && active;
  return (
    <button
      onClick={() => onActivate(state)}
      className={`flex flex-col justify-between rounded-2xl p-4 text-left transition-colors ${
        accent ? 'bg-bt-blue text-bt-charcoal' : 'bg-black/30 text-bt-off-white active:bg-black/50'
      }`}>
      <div className="text-2xl">{domainIcon(state.entityId)}</div>
      <div className="truncate text-sm font-medium">{friendlyName(state)}</div>
      <div className={`text-xs ${accent ? 'text-bt-charcoal/70' : 'text-bt-soft-gray'}`}>
        {actionLabel(state, kind)}
      </div>
    </button>
  );
}

function ClimateTile({ tile, state, onSetTemp }: { tile: Tile; state: HaState; onSetTemp: Props['onSetTemp'] }) {
  const current = num(state.attributes['current_temperature']);
  const target = tile.pendingTemp ?? num(state.attributes['temperature']);
  const step = num(state.attributes['target_temp_step']) ?? 0.5;
  const min = num(state.attributes['min_temp']) ?? 7;
  const max = num(state.attributes['max_temp']) ?? 35;
  const adjust = (delta: number) => {
    if (target == null) return;
    onSetTemp(state.entityId, clamp(round(target + delta, step), min, max));
  };
  return (
    <div className="flex flex-col justify-between rounded-2xl bg-black/30 p-4">
      <div className="flex items-center justify-between">
        <span className="text-2xl">{domainIcon(state.entityId)}</span>
        <span className="text-xs text-bt-soft-gray">{state.state}</span>
      </div>
      <div className="truncate text-sm font-medium">{friendlyName(state)}</div>
      <div className="flex items-center justify-between">
        <button
          onClick={() => adjust(-step)}
          className="h-9 w-9 rounded-full bg-black/40 text-lg active:bg-black/60"
          disabled={target == null}>
          -
        </button>
        <div className="text-center">
          <div className="bt-wordmark text-2xl font-semibold leading-none">
            {target != null ? `${fmt(target)}°` : '--'}
          </div>
          {current != null && <div className="text-[0.65rem] text-bt-soft-gray">now {fmt(current)}°</div>}
        </div>
        <button
          onClick={() => adjust(step)}
          className="h-9 w-9 rounded-full bg-black/40 text-lg active:bg-black/60"
          disabled={target == null}>
          +
        </button>
      </div>
    </div>
  );
}

function ReadonlyTile({ state }: { state: HaState }) {
  const unit =
    typeof state.attributes['unit_of_measurement'] === 'string'
      ? (state.attributes['unit_of_measurement'] as string)
      : '';
  return (
    <div className="flex flex-col justify-between rounded-2xl bg-black/20 p-4">
      <div className="text-2xl">{domainIcon(state.entityId)}</div>
      <div className="truncate text-sm font-medium">{friendlyName(state)}</div>
      <div className="bt-wordmark text-lg">
        {state.state}
        {unit && <span className="ml-0.5 text-xs text-bt-soft-gray">{unit}</span>}
      </div>
    </div>
  );
}

function FullError({ message }: { message: string }) {
  return (
    <div className="flex h-full w-full items-center justify-center bg-bt-charcoal px-10">
      <div className="max-w-[34rem] text-center text-sm text-bt-soft-gray">{message}</div>
    </div>
  );
}

function actionLabel(state: HaState, kind: 'toggle' | 'lock' | 'momentary'): string {
  if (kind === 'momentary') return 'tap to run';
  return state.state;
}

function statusLabel(status: HaStatus): string {
  if (status.kind === 'error') return status.message;
  if (status.kind === 'connecting') return 'reconnecting...';
  if (status.kind === 'authenticating') return 'authenticating...';
  return '';
}

function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}

function round(v: number, step: number): number {
  return Math.round(v / step) * step;
}

function fmt(v: number): string {
  return Number.isInteger(v) ? String(v) : v.toFixed(1);
}
