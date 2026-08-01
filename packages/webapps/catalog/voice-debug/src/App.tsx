import {
  BridgethingClient,
  type NluSlots,
  type VoiceActivity,
  type VoiceIntent,
  type VoiceState,
} from '@bridgething/client';
import { useEffect, useMemo, useRef, useState } from 'react';

const wsUrl =
  import.meta.env.VITE_BRIDGETHING_URL ??
  (typeof window !== 'undefined' ? `ws://${window.location.host}/` : 'ws://127.0.0.1:8891/');

const HISTORY_MAX = 40;

type Turn = {
  key: string;
  at: number;
  activity: VoiceActivity;
  display?: VoiceIntent;
};

const PHASE_TONE: Record<string, string> = {
  idle: 'bg-white/10 text-white/50',
  listening: 'bg-sky-500/20 text-sky-300',
  thinking: 'bg-amber-500/20 text-amber-300',
  done: 'bg-emerald-500/20 text-emerald-300',
  failed: 'bg-rose-500/20 text-rose-300',
};

const STAGE_TONE: Record<string, string> = {
  fastPath: 'bg-emerald-500/15 text-emerald-300',
  model: 'bg-violet-500/15 text-violet-300',
  rejectedNoIntent: 'bg-white/10 text-white/50',
  rejectedClarify: 'bg-amber-500/15 text-amber-300',
  noModel: 'bg-rose-500/15 text-rose-300',
};

export default function App() {
  const client = useMemo(() => new BridgethingClient({ url: wsUrl }), []);
  const [state, setState] = useState<VoiceState>({ muted: false, capturing: false, phase: 'idle' });
  const [live, setLive] = useState<VoiceActivity | null>(null);
  const [turns, setTurns] = useState<Turn[]>([]);
  const seq = useRef(0);

  useEffect(() => {
    const push = (activity: VoiceActivity) => {
      setLive(activity.phase === 'done' || activity.phase === 'failed' ? null : activity);
      if (activity.phase !== 'done' && activity.phase !== 'failed') return;
      setTurns(prev => [{ key: `t${seq.current++}`, at: Date.now(), activity }, ...prev].slice(0, HISTORY_MAX));
    };

    const offActivity = client.voice.onActivity(push);
    const offState = client.voice.onStateChanged(setState);
    const offIntent = client.voice.onIntent(intent => {
      setTurns(prev => (prev.length === 0 ? prev : [{ ...prev[0], display: intent }, ...prev.slice(1)]));
    });

    void client.voice.stateGet().then(result => {
      if (result.ok) setState(result.response.state);
    });

    return () => {
      offActivity();
      offState();
      offIntent();
    };
  }, [client]);

  const current = live ?? turns[0]?.activity ?? null;

  return (
    <div className="flex h-full w-full flex-col bg-[#0d0f12] text-white">
      <header className="flex items-center justify-between gap-3 border-b border-white/10 px-4 py-2">
        <div className="flex items-center gap-2">
          <Chip tone={PHASE_TONE[state.phase] ?? PHASE_TONE.idle}>{state.phase}</Chip>
          {state.muted && <Chip tone="bg-rose-500/20 text-rose-300">mic muted</Chip>}
          {state.capturing && <Chip tone="bg-sky-500/20 text-sky-300">capturing</Chip>}
        </div>
        <div className="flex gap-2">
          <button
            className="rounded-lg bg-white/10 px-3 py-1.5 text-sm active:bg-white/20"
            onClick={() => void client.voice.pushToTalk()}>
            talk
          </button>
          <button
            className="rounded-lg bg-white/10 px-3 py-1.5 text-sm active:bg-white/20"
            onClick={() => void client.voice.cancel()}>
            cancel
          </button>
          <button
            className="rounded-lg bg-white/10 px-3 py-1.5 text-sm active:bg-white/20"
            onClick={() =>
              void (state.muted
                ? client.voice.unmuteMic({ preserve: false })
                : client.voice.muteMic({ preserve: false }))
            }>
            {state.muted ? 'unmute' : 'mute'}
          </button>
          <button
            className="rounded-lg bg-white/10 px-3 py-1.5 text-sm active:bg-white/20"
            onClick={() => setTurns([])}>
            clear
          </button>
        </div>
      </header>

      <section className="border-b border-white/10 px-4 py-3">
        {current ? <Detail activity={current} /> : <p className="text-sm text-white/40">waiting for a wake word</p>}
      </section>

      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-2">
        {turns.length === 0 ? (
          <p className="py-6 text-center text-sm text-white/30">no turns yet</p>
        ) : (
          <ul className="flex flex-col gap-1.5">
            {turns.map(turn => (
              <li key={turn.key} className="rounded-lg bg-white/5 px-3 py-2">
                <div className="flex items-center gap-2 text-xs">
                  <span className="tabular-nums text-white/35">{clock(turn.at)}</span>
                  <Chip tone={PHASE_TONE[turn.activity.phase] ?? PHASE_TONE.idle}>{turn.activity.phase}</Chip>
                  {turn.activity.stage && (
                    <Chip tone={STAGE_TONE[turn.activity.stage] ?? 'bg-white/10 text-white/50'}>
                      {turn.activity.stage}
                    </Chip>
                  )}
                  <span className="font-mono text-white/70">{turn.activity.intent ?? '-'}</span>
                  {turn.activity.target && <span className="text-white/35">to {turn.activity.target}</span>}
                  {turn.display && <Chip tone="bg-indigo-500/15 text-indigo-300">rendered {turn.display.intent}</Chip>}
                </div>
                {turn.activity.transcript && (
                  <p className="mt-1 truncate text-sm text-white/80">&ldquo;{turn.activity.transcript}&rdquo;</p>
                )}
                {turn.activity.error && (
                  <p className="mt-1 text-xs text-rose-300/80">
                    {turn.activity.error.code}: {turn.activity.error.msg}
                  </p>
                )}
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

function Detail({ activity }: { activity: VoiceActivity }) {
  const slots = slotPairs(activity.slots);
  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2 text-xs">
        <Chip tone={PHASE_TONE[activity.phase] ?? PHASE_TONE.idle}>{activity.phase}</Chip>
        {activity.reason && <Chip tone="bg-white/10 text-white/60">{activity.reason}</Chip>}
        {activity.score != null && <Chip tone="bg-white/10 text-white/60">score {activity.score.toFixed(3)}</Chip>}
        {activity.stage && (
          <Chip tone={STAGE_TONE[activity.stage] ?? 'bg-white/10 text-white/50'}>{activity.stage}</Chip>
        )}
        {activity.target && <Chip tone="bg-white/10 text-white/60">{activity.target}</Chip>}
      </div>
      <p className="text-lg leading-tight">
        {activity.transcript ? (
          <span>&ldquo;{activity.transcript}&rdquo;</span>
        ) : (
          <span className="text-white/35">{activity.phase === 'listening' ? 'listening...' : 'no transcript'}</span>
        )}
      </p>
      <div className="flex items-center gap-2 text-sm">
        <span className="font-mono text-white/80">{activity.intent ?? '-'}</span>
        {slots.length > 0 && (
          <span className="flex flex-wrap gap-1">
            {slots.map(([key, value]) => (
              <span key={key} className="rounded bg-white/8 px-1.5 py-0.5 font-mono text-xs text-white/60">
                {key}={value}
              </span>
            ))}
          </span>
        )}
      </div>
      {activity.error && (
        <p className="text-sm text-rose-300/90">
          {activity.error.code}: {activity.error.msg}
        </p>
      )}
    </div>
  );
}

function Chip({ tone, children }: { tone: string; children: React.ReactNode }) {
  return <span className={`rounded px-1.5 py-0.5 text-xs ${tone}`}>{children}</span>;
}

function slotPairs(slots: NluSlots | undefined): [string, string][] {
  if (!slots) return [];
  return Object.entries(slots)
    .filter(([, value]) => value !== null && value !== undefined)
    .map(([key, value]) => [key, String(value)]);
}

function clock(at: number): string {
  return new Date(at).toLocaleTimeString('en-US', { hour12: false });
}
