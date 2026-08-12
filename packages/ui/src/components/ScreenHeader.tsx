import type { ComponentChildren, VNode } from 'preact';

import { cx } from '../cx.ts';

export function ScreenHeader({
  eyebrow,
  title,
  subtitle,
  trailing,
  class: className,
}: {
  eyebrow?: ComponentChildren;
  title: ComponentChildren;
  subtitle?: ComponentChildren;
  trailing?: ComponentChildren;
  class?: string;
}): VNode {
  return (
    <div class={cx('mb-6 flex items-end justify-between gap-4', className)}>
      <div class="flex min-w-0 flex-1 flex-col">
        {eyebrow ? (
          <span class="mb-1.5 font-mono text-eyebrow tracking-[0.18em] text-accent uppercase">{eyebrow}</span>
        ) : null}
        <h1 class="m-0 font-display text-screen-title font-medium tracking-wordmark wrap-break-word">{title}</h1>
        {subtitle ? <span class="mt-2 text-body wrap-break-word text-muted">{subtitle}</span> : null}
      </div>
      {trailing ? <div class="shrink-0 pb-1">{trailing}</div> : null}
    </div>
  );
}
