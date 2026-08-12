import { cx } from '@bridgething/ui';
import type { VNode } from 'preact';

export function Progress({ percent, class: className }: { percent: number | null; class?: string }): VNode {
  if (percent === null) {
    return (
      <div class={cx('h-1 overflow-hidden bg-neutral-soft', className)}>
        <div class="h-full w-1/3 animate-[bridgething-sweep_1.4s_ease-in-out_infinite] bg-accent" />
      </div>
    );
  }

  return (
    <div
      class={cx('h-1 bg-neutral-soft', className)}
      role="progressbar"
      aria-valuenow={percent}
      aria-valuemin={0}
      aria-valuemax={100}>
      <div class="h-full bg-accent transition-[width] duration-300" style={{ width: `${percent}%` }} />
    </div>
  );
}
