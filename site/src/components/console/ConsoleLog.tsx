import { useEffect, useRef } from 'preact/hooks';
import type { LogKind, LogLine } from './useConsoleLog';

const TS_COLOR: Record<LogKind, string> = {
  info: 'text-accent/55',
  ok: 'text-ok',
  warn: 'text-warn',
  err: 'text-err',
};

export function ConsoleLog({ title, lines }: { title: string; lines: LogLine[] }) {
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const node = box.current;
    if (node) node.scrollTop = node.scrollHeight;
  }, [lines]);

  return (
    <div class="border border-white/20 bg-black shadow-[0_0_80px_rgba(0,0,0,0.6)]">
      <div class="flex items-center gap-1.5 border-b border-white/10 px-4 py-2.5">
        <div class="size-2 rounded-full bg-white/10" />
        <div class="size-2 rounded-full bg-white/10" />
        <div class="size-2 rounded-full bg-white/10" />
        <p class="m-0 ml-2 font-mono text-xs text-white/35">{title}</p>
      </div>
      <div
        ref={box}
        class="flex max-h-64 flex-col gap-1.5 overflow-y-auto p-5 font-mono text-base whitespace-nowrap max-sm:text-sm">
        {lines.map(line => (
          <p key={line.id} class="m-0 text-white/60">
            <span class={`whitespace-pre ${TS_COLOR[line.kind]}`}>
              {`[${(line.at / 1000).toFixed(6).padStart(12)}] `}
            </span>
            {line.message}
          </p>
        ))}
      </div>
    </div>
  );
}
