import type { VNode } from 'preact';

import { cx } from '../cx.ts';

export type WordmarkSize = 'xs' | 'sm' | 'md' | 'lg' | 'xl';

const SIZE: Record<WordmarkSize, string> = {
  xs: 'text-[0.875rem]',
  sm: 'text-[1.125rem]',
  md: 'text-[1.75rem]',
  lg: 'text-[2.75rem]',
  xl: 'text-[4rem]',
};

export function Wordmark({
  as: Tag = 'span',
  size = 'md',
  class: className,
}: {
  as?: 'span' | 'div' | 'h1' | 'h2';
  size?: WordmarkSize;
  class?: string;
}): VNode {
  return (
    <Tag class={cx('m-0 inline-block font-display leading-none tracking-wordmark', SIZE[size], className)}>
      <span class="font-medium">bridge</span>
      <span class="font-extralight">thing</span>
    </Tag>
  );
}
