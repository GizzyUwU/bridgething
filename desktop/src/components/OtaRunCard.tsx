import type { OtaRun } from '@bridgething/companion-types';
import { Button, Pill } from '@bridgething/ui';
import type { VNode } from 'preact';

import { bytes, rate } from '../lib/format.ts';
import { isRunning, phaseWord, runTitle, runTone, stepIndex, stepLabel, stepPercent } from '../lib/ota.ts';
import { Progress } from './Progress.tsx';

export function OtaRunCard({ run, onDismiss }: { run: OtaRun; onDismiss?: () => void }): VNode {
  const tone = runTone(run);
  const live = isRunning(run);
  const percent = stepPercent(run);
  const label = stepLabel(run);
  const moving = rate(run.ratePerSec);

  return (
    <div class="border border-rule bg-screen">
      <div class="flex items-start gap-3 px-4 py-3">
        <div class="flex min-w-0 flex-1 flex-col gap-1">
          <div class="flex items-center gap-2">
            <span class="truncate text-row text-off-white">{runTitle(run)}</span>
            <Pill tone={tone} dot={live}>
              {phaseWord(run)}
            </Pill>
          </div>
          <span class="truncate text-hint text-muted">
            {label ?? 'preparing'}
            {run.steps.length > 0 ? ` · step ${stepIndex(run) + 1} of ${run.steps.length}` : ''}
            {moving ? ` · ${moving}` : ''}
          </span>
          {run.stageTotal !== null && run.stageTotal > 0 ? (
            <span class="font-mono text-hint text-dim">
              {bytes(run.stageReceived ?? 0)} of {bytes(run.stageTotal)}
            </span>
          ) : null}
        </div>
        <div class="flex shrink-0 items-center gap-2">
          {percent !== null ? (
            <span class="font-mono text-body tabular-nums text-soft transition-colors duration-300">{percent}%</span>
          ) : null}
          {!live && onDismiss ? (
            <Button size="sm" variant="ghost" onClick={onDismiss}>
              dismiss
            </Button>
          ) : null}
        </div>
      </div>
      {live ? <Progress percent={percent} /> : null}
      {run.error ? <p class="border-t border-rule px-4 py-2 text-hint text-err">{run.error}</p> : null}
    </div>
  );
}
