import { memo, useCallback, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { domainOf, friendlyName, isControllable, isDefaultPick } from './domains';
import type { HaState } from './ha';
import { DomainIcon } from './icons';

const DEFAULT_CAP = 12;

export const PICKER_COLS = 2;
const ROW_HEIGHT = 56;
const ROW_GAP = 8;
export const PICKER_ROW_PITCH = ROW_HEIGHT + ROW_GAP;
const OVERSCAN_ROWS = 3;

export function visibleRange(count: number, scrollTop: number, viewportHeight: number): { start: number; end: number } {
  const rows = Math.ceil(count / PICKER_COLS);
  const firstRow = clamp(Math.floor(scrollTop / PICKER_ROW_PITCH) - OVERSCAN_ROWS, 0, Math.max(0, rows - 1));
  const lastRow = clamp(Math.ceil((scrollTop + viewportHeight) / PICKER_ROW_PITCH) + OVERSCAN_ROWS, firstRow, rows);
  return { start: firstRow * PICKER_COLS, end: Math.min(count, lastRow * PICKER_COLS) };
}

function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}

type Props = {
  all: HaState[];
  initial: string[];
  onDone: (ids: string[]) => void;
  onCancel?: () => void;
};

export default function Picker({ all, initial, onDone, onCancel }: Props) {
  const sorted = useMemo(() => [...all].sort(compareEntities), [all]);
  const domains = useMemo(() => uniqueDomains(sorted), [sorted]);

  const [selected, setSelected] = useState<Set<string>>(() => initialSelection(sorted, initial));
  const [domainFilter, setDomainFilter] = useState<string>('all');

  const listRef = useRef<HTMLDivElement | null>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(0);

  const visible = useMemo(
    () => (domainFilter === 'all' ? sorted : sorted.filter(s => domainOf(s.entityId) === domainFilter)),
    [sorted, domainFilter],
  );

  useLayoutEffect(() => {
    const el = listRef.current;
    if (!el) return;
    setViewportHeight(el.clientHeight);
    const observer = new ResizeObserver(() => setViewportHeight(el.clientHeight));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const toggle = useCallback((id: string) => {
    setSelected(prev => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const filterBy = useCallback((domain: string) => {
    setDomainFilter(domain);
    setScrollTop(0);
    if (listRef.current) listRef.current.scrollTop = 0;
  }, []);

  const { start, end } = visibleRange(visible.length, scrollTop, viewportHeight);

  return (
    <div className="flex h-full w-full flex-col bg-bg text-off-white">
      <header className="mb-2 flex items-center justify-between border-b border-rule px-6 pt-4 pb-2">
        <div className="flex items-baseline gap-3">
          <span className="font-mono text-eyebrow tracking-[0.25em] text-dim uppercase">choose tiles</span>
          <span className="font-mono text-hint tabular-nums text-accent">{selected.size} selected</span>
        </div>
        <div className="flex gap-2">
          {onCancel && (
            <button
              onClick={onCancel}
              className="border border-rule px-4 py-1.5 font-mono text-eyebrow text-soft active:bg-neutral-soft">
              cancel
            </button>
          )}
          <button
            onClick={() => onDone([...selected])}
            disabled={selected.size === 0}
            className="border border-accent bg-accent px-6 py-1.5 font-mono text-eyebrow text-screen disabled:border-rule disabled:bg-transparent disabled:text-dim">
            done
          </button>
        </div>
      </header>

      <div className="flex shrink-0 gap-2 overflow-x-auto px-6 pb-2">
        <Chip label="all" active={domainFilter === 'all'} onClick={() => filterBy('all')} />
        {domains.map(d => (
          <Chip key={d} label={d} active={domainFilter === d} onClick={() => filterBy(d)} />
        ))}
      </div>

      <div
        ref={listRef}
        onScroll={e => setScrollTop(e.currentTarget.scrollTop)}
        className="flex-1 overflow-y-auto px-6 pb-5">
        <div
          className="relative"
          style={{ height: Math.max(0, Math.ceil(visible.length / PICKER_COLS) * PICKER_ROW_PITCH - ROW_GAP) }}>
          {visible.slice(start, end).map((s, i) => (
            <EntityRow
              key={s.entityId}
              state={s}
              index={start + i}
              selected={selected.has(s.entityId)}
              onToggle={toggle}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

type RowProps = { state: HaState; index: number; selected: boolean; onToggle: (id: string) => void };

const EntityRow = memo(function EntityRow({ state, index, selected, onToggle }: RowProps) {
  const row = Math.floor(index / PICKER_COLS);
  const col = index % PICKER_COLS;
  return (
    <button
      onClick={() => onToggle(state.entityId)}
      style={{
        top: row * PICKER_ROW_PITCH,
        height: ROW_HEIGHT,
        left: col === 0 ? 0 : `calc(50% + ${ROW_GAP / 2}px)`,
        width: `calc(50% - ${ROW_GAP / 2}px)`,
      }}
      className={`absolute flex items-center gap-3 border px-3 text-left transition-colors ${
        selected ? 'border-accent bg-accent-soft text-accent' : 'border-rule bg-screen active:bg-neutral-soft'
      }`}>
      <DomainIcon entityId={state.entityId} size={20} />
      <span className="min-w-0 flex-1">
        <span className="block truncate text-row font-medium">{friendlyName(state)}</span>
        <span className="block truncate font-mono text-hint text-dim">{state.entityId}</span>
      </span>
      <span className={`font-mono text-row ${selected ? 'text-accent' : 'text-dim'}`}>{selected ? '[x]' : '[ ]'}</span>
    </button>
  );
});

function Chip({ label, active, onClick }: { label: string; active: boolean; onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      className={`shrink-0 border px-3 py-1 font-mono text-eyebrow tracking-[0.08em] ${active ? 'border-off-white bg-off-white text-screen' : 'border-rule text-soft active:bg-neutral-soft'}`}>
      {label}
    </button>
  );
}

function initialSelection(sorted: HaState[], initial: string[]): Set<string> {
  if (initial.length) return new Set(initial);
  const defaults = sorted.filter(s => isDefaultPick(s.entityId)).slice(0, DEFAULT_CAP);
  return new Set(defaults.map(s => s.entityId));
}

function uniqueDomains(states: HaState[]): string[] {
  const set = new Set(states.map(s => domainOf(s.entityId)));
  return [...set].sort();
}

function compareEntities(a: HaState, b: HaState): number {
  const ca = isControllable(a.entityId) ? 0 : 1;
  const cb = isControllable(b.entityId) ? 0 : 1;
  if (ca !== cb) return ca - cb;
  const da = domainOf(a.entityId);
  const db = domainOf(b.entityId);
  if (da !== db) return da < db ? -1 : 1;
  return friendlyName(a).localeCompare(friendlyName(b));
}
