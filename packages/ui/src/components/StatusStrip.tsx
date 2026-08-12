import type { ComponentChildren, VNode } from 'preact';

import { cx } from '../cx.ts';
import { TONE_DOT, TONE_EDGE, TONE_FILL, type Tone } from '../tokens.ts';

export function StatusStrip({
  tone = 'neutral',
  title,
  subtitle,
  onClick,
  class: className,
}: {
  tone?: Tone;
  title: ComponentChildren;
  subtitle?: ComponentChildren;
  onClick?: () => void;
  class?: string;
}): VNode {
  const body = (
    <>
      <span aria-hidden="true" class={cx('size-2 shrink-0', TONE_DOT[tone])} />
      <span class="flex min-w-0 flex-1 flex-col gap-0.5 text-left">
        <span class="truncate text-body text-off-white">{title}</span>
        {subtitle ? <span class="truncate text-hint text-muted">{subtitle}</span> : null}
      </span>
      {onClick ? (
        <span aria-hidden="true" class="shrink-0 font-mono text-body text-dim">
          &rsaquo;
        </span>
      ) : null}
    </>
  );

  const shape = cx(
    'flex w-full items-center gap-3 border px-4 py-3',
    TONE_FILL[tone],
    TONE_EDGE[tone],
    onClick &&
      'transition-colors hover:border-current focus-visible:outline-2 focus-visible:outline-accent focus-visible:-outline-offset-2',
    className,
  );

  if (!onClick) return <div class={shape}>{body}</div>;

  return (
    <button type="button" class={shape} onClick={onClick}>
      {body}
    </button>
  );
}
