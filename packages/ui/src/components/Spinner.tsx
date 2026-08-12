import type { VNode } from 'preact';

import { cx } from '../cx.ts';

export function Spinner({ class: className }: { class?: string }): VNode {
  return (
    <span
      aria-hidden="true"
      class={cx(
        'inline-block size-3.5 shrink-0 animate-spin rounded-full border border-current border-t-transparent',
        className,
      )}
    />
  );
}
