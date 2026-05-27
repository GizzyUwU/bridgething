import { BridgethingClient, type NetFetchReply } from '@bridgething/client';
import { useEffect, useMemo, useState } from 'react';

const wsUrl =
  import.meta.env.VITE_BRIDGETHING_URL ??
  (typeof window !== 'undefined' ? `ws://${window.location.host}/` : 'ws://127.0.0.1:8891/');

type CalEvent = { start: Date; allDay: boolean; title: string; location: string | null };

type Phase = { kind: 'loading' } | { kind: 'ready'; events: CalEvent[] } | { kind: 'error'; message: string };

export default function App() {
  const client = useMemo(() => new BridgethingClient({ url: wsUrl }), []);
  const [phase, setPhase] = useState<Phase>({ kind: 'loading' });
  const [now, setNow] = useState<Date>(new Date());

  useEffect(() => {
    let cancelled = false;

    const load = async () => {
      if (!cancelled) setPhase({ kind: 'loading' });
      try {
        const t = await client.time.get();
        if (t.ok && t.response.time.wallClockUnixS && !cancelled) {
          setNow(new Date(t.response.time.wallClockUnixS * 1000));
        }

        const cfg = await client.config.get({ key: 'ics_url' });
        const url = cfg.ok ? cfg.response.value : null;
        if (!url) {
          if (!cancelled)
            setPhase({ kind: 'error', message: 'set "ics_url" in the companion app to a public iCalendar feed.' });
          return;
        }

        const res = await client.net.fetch({
          request: { url, method: 'GET', headers: [], body: null, timeoutMs: 15_000, redirect: 'follow' },
        });
        if (!res.ok) throw new Error(res.kind === 'domain' ? 'no network — is a phone connected?' : 'fetch failed.');
        const reply = res.response as NetFetchReply;
        if (reply.response.status >= 400) throw new Error(`calendar feed returned ${reply.response.status}`);

        const text = new TextDecoder().decode(new Uint8Array(reply.response.body as unknown as number[]));
        const startOfToday = new Date();
        startOfToday.setHours(0, 0, 0, 0);
        const events = parseIcs(text)
          .filter(e => e.start >= startOfToday)
          .sort((a, b) => a.start.getTime() - b.start.getTime())
          .slice(0, 24);
        if (!cancelled) setPhase({ kind: 'ready', events });
      } catch (err) {
        if (!cancelled) setPhase({ kind: 'error', message: err instanceof Error ? err.message : String(err) });
      }
    };

    load();
    const offChanged = client.config.onChanged(() => load());
    return () => {
      cancelled = true;
      offChanged();
    };
  }, [client]);

  return (
    <div className="flex h-full w-full flex-col bg-bt-charcoal text-bt-off-white">
      <header className="flex items-baseline justify-between px-8 pt-6 pb-3">
        <div className="bt-wordmark text-2xl font-semibold">
          {now.toLocaleDateString(undefined, { weekday: 'long', month: 'long', day: 'numeric' })}
        </div>
        <div className="text-sm text-bt-soft-gray">upcoming</div>
      </header>
      <main className="flex-1 overflow-y-auto px-8 pb-6">
        {phase.kind === 'loading' && <Note>loading calendar...</Note>}
        {phase.kind === 'error' && <Note>{phase.message}</Note>}
        {phase.kind === 'ready' &&
          (phase.events.length === 0 ? <Note>nothing coming up.</Note> : <Agenda events={phase.events} />)}
      </main>
    </div>
  );
}

function Agenda({ events }: { events: CalEvent[] }) {
  const groups = groupByDay(events);
  return (
    <div className="flex flex-col gap-4">
      {groups.map(g => (
        <div key={g.key} className="flex gap-4">
          <div className="w-28 shrink-0 pt-1 text-right">
            <div className="text-xs uppercase tracking-wide text-bt-soft-gray">{g.weekday}</div>
            <div className="bt-wordmark text-xl font-medium">{g.dayLabel}</div>
          </div>
          <div className="flex flex-1 flex-col gap-2">
            {g.events.map((e, i) => (
              <div key={i} className="flex items-baseline gap-3 rounded-xl bg-black/30 px-4 py-2.5">
                <div className="w-16 shrink-0 text-sm tabular-nums text-bt-blue">
                  {e.allDay ? 'all day' : e.start.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })}
                </div>
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm font-medium text-bt-off-white">{e.title}</div>
                  {e.location && <div className="truncate text-xs text-bt-soft-gray">{e.location}</div>}
                </div>
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

function Note({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-full items-center justify-center">
      <div className="max-w-[34rem] text-center text-sm text-bt-soft-gray">{children}</div>
    </div>
  );
}

function groupByDay(events: CalEvent[]) {
  const groups: { key: string; weekday: string; dayLabel: string; events: CalEvent[] }[] = [];
  for (const e of events) {
    const key = e.start.toDateString();
    let g = groups.find(x => x.key === key);
    if (!g) {
      g = {
        key,
        weekday: e.start.toLocaleDateString(undefined, { weekday: 'short' }),
        dayLabel: e.start.toLocaleDateString(undefined, { month: 'short', day: 'numeric' }),
        events: [],
      };
      groups.push(g);
    }
    g.events.push(e);
  }
  return groups;
}

function parseIcs(raw: string): CalEvent[] {
  const lines = unfold(raw);
  const events: CalEvent[] = [];
  let cur: { start?: Date; allDay?: boolean; title?: string; location?: string } | null = null;
  for (const line of lines) {
    if (line === 'BEGIN:VEVENT') {
      cur = {};
      continue;
    }
    if (line === 'END:VEVENT') {
      if (cur?.start) {
        events.push({
          start: cur.start,
          allDay: cur.allDay ?? false,
          title: cur.title ?? '(untitled)',
          location: cur.location ?? null,
        });
      }
      cur = null;
      continue;
    }
    if (!cur) continue;
    const colon = line.indexOf(':');
    if (colon < 0) continue;
    const left = line.slice(0, colon);
    const value = line.slice(colon + 1);
    const name = left.split(';')[0].toUpperCase();
    if (name === 'SUMMARY') cur.title = unescapeIcs(value);
    else if (name === 'LOCATION') cur.location = unescapeIcs(value);
    else if (name === 'DTSTART') {
      const parsed = parseDt(left, value);
      cur.start = parsed.date;
      cur.allDay = parsed.allDay;
    }
  }
  return events;
}

function unfold(raw: string): string[] {
  const out: string[] = [];
  for (const line of raw.split(/\r?\n/)) {
    if ((line.startsWith(' ') || line.startsWith('\t')) && out.length) out[out.length - 1] += line.slice(1);
    else out.push(line);
  }
  return out;
}

// TZID-qualified times are treated as device-local; good enough for a sample.
function parseDt(left: string, value: string): { date: Date; allDay: boolean } {
  if (/VALUE=DATE\b/i.test(left) || /^\d{8}$/.test(value)) {
    return { date: new Date(+value.slice(0, 4), +value.slice(4, 6) - 1, +value.slice(6, 8)), allDay: true };
  }
  const m = value.match(/^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})(Z)?$/);
  if (!m) return { date: new Date(value), allDay: false };
  const [, y, mo, d, h, mi, s, z] = m;
  if (z) return { date: new Date(Date.UTC(+y, +mo - 1, +d, +h, +mi, +s)), allDay: false };
  return { date: new Date(+y, +mo - 1, +d, +h, +mi, +s), allDay: false };
}

function unescapeIcs(v: string): string {
  return v.replace(/\\n/gi, '\n').replace(/\\,/g, ',').replace(/\\;/g, ';').replace(/\\\\/g, '\\');
}
