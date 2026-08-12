import type { ComponentChildren, VNode } from 'preact';
import { useId } from 'preact/hooks';

import { cx } from '../cx.ts';

export type SegmentedOption<T extends string> = { value: T; label: ComponentChildren };

export type Segment<T extends string> = { value: T; label: ComponentChildren; selected: boolean };

export function segments<T extends string>(
  options: readonly T[] | readonly SegmentedOption<T>[],
  value: T,
): Segment<T>[] {
  return (options as readonly (T | SegmentedOption<T>)[]).map(option =>
    typeof option === 'string'
      ? { value: option, label: option, selected: option === value }
      : { value: option.value, label: option.label, selected: option.value === value },
  );
}

export function Segmented<T extends string>({
  options,
  value,
  onChange,
  label,
  size = 'md',
  disabled = false,
  class: className,
}: {
  options: readonly T[] | readonly SegmentedOption<T>[];
  value: T;
  onChange: (next: T) => void;
  label: string;
  size?: 'sm' | 'md';
  disabled?: boolean;
  class?: string;
}): VNode {
  const name = useId();
  const items = segments(options, value);
  const shape = size === 'sm' ? 'px-2.5 py-1 text-eyebrow' : 'px-3 py-1.5 text-hint';

  return (
    <div
      role="radiogroup"
      aria-label={label}
      class={cx('inline-flex border border-rule', disabled && 'opacity-40', className)}>
      {items.map((item, index) => (
        <label key={item.value} class={cx('flex', index > 0 && 'border-l border-rule')}>
          <input
            type="radio"
            name={name}
            value={item.value}
            checked={item.selected}
            disabled={disabled}
            class="peer sr-only"
            onChange={() => onChange(item.value)}
          />
          <span
            class={cx(
              'font-mono whitespace-nowrap text-soft uppercase transition-colors peer-checked:bg-accent-soft peer-checked:text-accent peer-focus-visible:outline-2 peer-focus-visible:outline-accent peer-focus-visible:-outline-offset-2 hover:text-off-white',
              shape,
            )}>
            {item.label}
          </span>
        </label>
      ))}
    </div>
  );
}
