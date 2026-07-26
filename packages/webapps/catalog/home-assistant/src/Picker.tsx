import { useMemo, useState } from 'react';
import { domainOf, friendlyName, isControllable, isDefaultPick } from './domains';
import type { HaState } from './ha';
import { DomainIcon } from './icons';

const DEFAULT_CAP = 12;

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

  const visible = domainFilter === 'all' ? sorted : sorted.filter(s => domainOf(s.entityId) === domainFilter);

  const toggle = (id: string) =>
    setSelected(prev => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  return (
    <div className="flex h-full w-full flex-col bg-bt-charcoal text-bt-off-white">
      <header className="flex items-center justify-between px-6 pt-4 pb-2">
        <div className="flex items-baseline gap-3">
          <span className="bt-wordmark text-xl font-semibold">Choose tiles</span>
          <span className="text-xs text-bt-soft-gray">{selected.size} selected</span>
        </div>
        <div className="flex gap-2">
          {onCancel && (
            <button
              onClick={onCancel}
              className="rounded-full bg-black/30 px-4 py-1.5 text-xs text-bt-soft-gray active:bg-black/50">
              cancel
            </button>
          )}
          <button
            onClick={() => onDone([...selected])}
            disabled={selected.size === 0}
            className="rounded-full bg-bt-blue px-5 py-1.5 text-xs font-medium text-bt-charcoal disabled:opacity-40">
            done
          </button>
        </div>
      </header>

      <div className="flex shrink-0 gap-2 overflow-x-auto px-6 pb-2">
        <Chip label="all" active={domainFilter === 'all'} onClick={() => setDomainFilter('all')} />
        {domains.map(d => (
          <Chip key={d} label={d} active={domainFilter === d} onClick={() => setDomainFilter(d)} />
        ))}
      </div>

      <div className="grid flex-1 grid-cols-2 content-start gap-2 overflow-y-auto px-6 pb-5">
        {visible.map(s => {
          const on = selected.has(s.entityId);
          return (
            <button
              key={s.entityId}
              onClick={() => toggle(s.entityId)}
              className={`flex items-center gap-3 rounded-xl px-3 py-2.5 text-left transition-colors ${
                on ? 'bg-bt-blue/20 ring-1 ring-bt-blue' : 'bg-black/25 active:bg-black/40'
              }`}>
              <DomainIcon entityId={s.entityId} size={20} />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm font-medium">{friendlyName(s)}</span>
                <span className="block truncate text-xs text-bt-soft-gray">{s.entityId}</span>
              </span>
              <span className={`text-sm ${on ? 'text-bt-blue' : 'text-bt-soft-gray'}`}>{on ? '✓' : '+'}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}

function Chip({ label, active, onClick }: { label: string; active: boolean; onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      className={`shrink-0 rounded-full px-3 py-1 text-xs ${active ? 'bg-bt-off-white text-bt-charcoal' : 'bg-black/30 text-bt-soft-gray active:bg-black/50'}`}>
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
