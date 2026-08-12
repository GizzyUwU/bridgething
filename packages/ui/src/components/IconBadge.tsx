import type { ComponentChildren, VNode } from 'preact';

import { cx } from '../cx.ts';
import { BOX, TONE_FILL, type BoxSize, type Tone } from '../tokens.ts';

export function IconBadge({
  tone = 'accent',
  size = 'md',
  class: className,
  children,
}: {
  tone?: Tone;
  size?: BoxSize;
  class?: string;
  children: ComponentChildren;
}): VNode {
  return (
    <span
      aria-hidden="true"
      class={cx('inline-flex shrink-0 items-center justify-center', BOX[size], TONE_FILL[tone], className)}>
      {children}
    </span>
  );
}
