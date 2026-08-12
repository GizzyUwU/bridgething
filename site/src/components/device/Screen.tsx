import { cx } from '@bridgething/ui';
import type { ComponentChildren, VNode } from 'preact';

export function Screen({ children }: { children: ComponentChildren }): VNode {
  return <div class="flex w-full flex-col gap-10">{children}</div>;
}

export function Section({ class: className, children }: { class?: string; children: ComponentChildren }): VNode {
  return <section class={cx('flex flex-col', className)}>{children}</section>;
}

export function ErrorNote({ children }: { children: ComponentChildren }): VNode {
  return <p class="border-err/30 bg-err-soft text-hint text-err mt-2 mb-0 border px-3 py-2">{children}</p>;
}

export function Hint({ children }: { children: ComponentChildren }): VNode {
  return <p class="text-hint text-muted mt-2 mb-0 leading-relaxed">{children}</p>;
}

export function Progress({ percent }: { percent: number }): VNode {
  return (
    <div class="bg-neutral-soft h-1" role="progressbar" aria-valuenow={percent} aria-valuemin={0} aria-valuemax={100}>
      <div class="bg-accent h-full transition-[width] duration-300" style={{ width: `${percent}%` }} />
    </div>
  );
}

export function bytes(count: number): string {
  if (count < 1024) return `${count} B`;
  if (count < 1024 * 1024) return `${(count / 1024).toFixed(1)} KiB`;
  return `${(count / (1024 * 1024)).toFixed(1)} MiB`;
}
