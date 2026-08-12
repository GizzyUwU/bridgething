import type { VNode } from 'preact';

import { cx } from '../cx.ts';

export function Switch({
  checked,
  onChange,
  label,
  disabled = false,
  class: className,
}: {
  checked: boolean;
  onChange: (next: boolean) => void;
  label: string;
  disabled?: boolean;
  class?: string;
}): VNode {
  return (
    <span class={cx('relative inline-flex h-5 w-9 shrink-0', disabled && 'opacity-40', className)}>
      <input
        type="checkbox"
        class="peer absolute inset-0 z-10 m-0 appearance-none opacity-0"
        checked={checked}
        disabled={disabled}
        aria-label={label}
        onChange={event => onChange(event.currentTarget.checked)}
      />
      <span
        aria-hidden="true"
        class="absolute inset-0 border border-edge bg-neutral-soft transition-colors peer-checked:border-accent peer-checked:bg-accent-soft peer-focus-visible:outline-2 peer-focus-visible:outline-accent peer-focus-visible:outline-offset-2"
      />
      <span
        aria-hidden="true"
        class="absolute inset-y-0.5 left-0.5 w-4 bg-dim transition-transform peer-checked:translate-x-4 peer-checked:bg-accent"
      />
    </span>
  );
}
