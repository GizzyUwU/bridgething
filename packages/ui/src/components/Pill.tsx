import type { ComponentChildren, VNode } from 'preact';

import { cx } from '../cx.ts';
import { TONE_DOT, TONE_FILL, type Tone } from '../tokens.ts';

export function Pill({
  tone = 'neutral',
  dot = false,
  class: className,
  children,
}: {
  tone?: Tone;
  dot?: boolean;
  class?: string;
  children: ComponentChildren;
}): VNode {
  return (
    <span
      class={cx(
        'inline-flex items-center gap-1.5 px-2 py-0.5 font-mono text-eyebrow whitespace-nowrap uppercase',
        TONE_FILL[tone],
        className,
      )}>
      {dot ? <span class={cx('size-1.5 shrink-0', TONE_DOT[tone])} /> : null}
      {children}
    </span>
  );
}
