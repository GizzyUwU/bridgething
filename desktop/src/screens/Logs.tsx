import type { DeviceLogLine, LogLevel } from '@bridgething/companion-types';
import { Button, Field, Pill, Segmented, Spinner, Switch, cx, describeError } from '@bridgething/ui';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import type { VNode } from 'preact';
import { useEffect, useState } from 'preact/hooks';

import { useDesktop } from '../desktop.ts';
import { clock } from '../lib/format.ts';
import { Icon } from '../lib/icons.tsx';
import { pickLogFile } from '../lib/picker.ts';
import { deviceLogsFor, logLimit, logStreaming } from '../stores/session.ts';

const LEVELS = ['all', 'debug', 'info', 'warn', 'error'] as const;
type LevelFilter = (typeof LEVELS)[number];

const SEVERITY: Record<LogLevel, number> = { trace: 0, debug: 1, info: 2, warn: 3, error: 4 };

const LIMITS = [
  { value: '500', label: '500' },
  { value: '2000', label: '2k' },
  { value: '10000', label: '10k' },
] as const;

const LEVEL_TINT: Record<LogLevel, string> = {
  trace: 'bg-neutral-soft text-dim',
  debug: 'bg-neutral-soft text-soft',
  info: 'bg-accent-soft text-accent',
  warn: 'bg-warn-soft text-warn',
  error: 'bg-err-soft text-err',
};

export function LogsScreen(): VNode {
  const session = useDesktop();
  const [level, setLevel] = useState<LevelFilter>('all');
  const [query, setQuery] = useState('');
  const [failure, setFailure] = useState<string | undefined>(undefined);
  const [copied, setCopied] = useState(false);
  const [saving, setSaving] = useState(false);

  const limit = logLimit.value;
  const lines = deviceLogsFor(limit);
  const streaming = logStreaming.data.value;
  const held = lines.data.value;
  const visible = filter(held, level, query);
  const note = failure ?? lines.error.value;

  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), 1500);
    return () => clearTimeout(timer);
  }, [copied]);

  const toggle = (next: boolean) => {
    void session.setDeviceLogStreaming(next);
  };

  const copy = async () => {
    try {
      await writeText(render(visible));
      setFailure(undefined);
      setCopied(true);
    } catch (reason) {
      setFailure(describeError(reason));
    }
  };

  const save = async () => {
    const path = await pickLogFile(`bridgething-${stamp()}.log`).catch(() => null);
    if (!path) return;
    setSaving(true);
    try {
      await session.exportLogs(path, render(visible));
      setFailure(undefined);
    } catch (reason) {
      setFailure(describeError(reason));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div class="flex min-h-0 min-w-0 flex-1 flex-col">
      <div class="flex shrink-0 flex-col gap-2.5 border-b border-rule bg-screen px-6 py-3">
        <div class="flex flex-wrap items-center gap-3">
          <Segmented options={LEVELS} value={level} label="minimum level" size="sm" onChange={setLevel} />
          <Segmented
            options={LIMITS}
            value={String(limit)}
            label="tail depth"
            size="sm"
            onChange={next => {
              logLimit.value = Number(next);
            }}
          />
          <label class="ml-auto flex items-center gap-2 font-mono text-eyebrow text-muted uppercase">
            device stream
            <Switch checked={streaming} label="stream device logs" onChange={toggle} />
          </label>
        </div>

        <div class="flex items-center gap-3">
          <Field
            class="flex-1"
            value={query}
            onInput={setQuery}
            icon={<Icon name="search" size={14} />}
            type="search"
            placeholder="filter messages and targets"
            clearable
          />
          <span class="shrink-0 font-mono text-hint text-muted">
            {visible.length === held.length ? `${held.length} lines` : `${visible.length} of ${held.length}`}
          </span>
          {lines.pending.value ? <Spinner /> : null}
          <Button
            variant="ghost"
            size="sm"
            icon={<Icon name={copied ? 'check' : 'copy'} size={14} />}
            disabled={visible.length === 0}
            onClick={() => void copy()}>
            {copied ? 'copied' : 'copy'}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            icon={<Icon name="download" size={14} />}
            loading={saving}
            disabled={visible.length === 0}
            onClick={() => void save()}>
            export
          </Button>
        </div>

        {!streaming ? (
          <span class="flex items-center gap-2">
            <Pill tone="neutral">stopped</Pill>
            <span class="text-hint text-muted">the device is not sending anything. host lines still land here.</span>
          </span>
        ) : null}
      </div>

      <div class="min-h-0 min-w-0 flex-1 overflow-y-auto px-6 py-3">
        {visible.length === 0 ? (
          <p class="py-12 text-center text-body text-muted">{emptyLine(held.length, streaming, query, level)}</p>
        ) : (
          <ol class="m-0 flex list-none flex-col p-0">
            {visible.map(line => (
              <li key={line.seq} class="min-w-0 border-b border-rule/60 py-1.5">
                <div class="flex min-w-0 items-center gap-2">
                  <span class={cx('shrink-0 px-1.5 font-mono text-eyebrow uppercase', LEVEL_TINT[line.level])}>
                    {line.level.slice(0, 4)}
                  </span>
                  <span class="shrink-0 font-mono text-eyebrow text-dim">{clock(line.tsUnixMs)}</span>
                  <span class="shrink-0 font-mono text-eyebrow text-dim uppercase">{line.origin}</span>
                  <span class="min-w-0 truncate font-mono text-eyebrow text-muted">{line.target}</span>
                </div>
                <p class="m-0 mt-0.5 font-mono text-hint leading-relaxed wrap-break-word whitespace-pre-wrap text-off-white">
                  {line.message}
                </p>
              </li>
            ))}
          </ol>
        )}
        {note ? <p class="mt-3 text-hint text-err">{note}</p> : null}
      </div>
    </div>
  );
}

function filter(lines: DeviceLogLine[], level: LevelFilter, query: string): DeviceLogLine[] {
  const floor = level === 'all' ? -1 : SEVERITY[level];
  const needle = query.trim().toLowerCase();
  return lines.filter(line => {
    if (floor >= 0 && SEVERITY[line.level] < floor) return false;
    if (!needle) return true;
    return line.message.toLowerCase().includes(needle) || line.target.toLowerCase().includes(needle);
  });
}

function render(lines: DeviceLogLine[]): string {
  return lines
    .map(
      line =>
        `${new Date(line.tsUnixMs).toISOString()} ${line.level.padEnd(5)} ${line.origin.padEnd(6)} ${line.target} ${line.message}`,
    )
    .join('\n');
}

function stamp(): string {
  return new Date().toISOString().replace(/[:.]/g, '-').replace(/Z$/, '');
}

function emptyLine(total: number, streaming: boolean, query: string, level: LevelFilter): string {
  if (total === 0) return streaming ? 'streaming, nothing has arrived yet' : 'turn the device stream on to see lines';
  if (query.trim()) return `no lines match "${query.trim()}"`;
  return `no lines at ${level} or above`;
}
