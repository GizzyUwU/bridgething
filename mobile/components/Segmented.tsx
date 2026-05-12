import { Text } from 'react-native';

import {
  ToggleGroup,
  ToggleGroupItem,
} from './ui/toggle-group';

/**
 * Pill-style segmented control. Built on RNR's ToggleGroup primitive
 * (single-mode = always one selected). Used for channel pickers, log-
 * level filters, enum config fields with a small choice count (~2-5).
 * For larger choice sets, fall back to a regular list.
 */
export function Segmented<T extends string>({
  options,
  value,
  onChange,
  size = 'md',
}: {
  options: ReadonlyArray<T> | ReadonlyArray<{ value: T; label: string }>;
  value: T;
  onChange: (next: T) => void;
  size?: 'sm' | 'md';
}) {
  const opts = options.map(o =>
    typeof o === 'string' ? { value: o as T, label: o } : o,
  );
  const txt = size === 'sm' ? 'text-[12px]' : 'text-[13px]';
  const itemPad = size === 'sm' ? 'px-3 py-1' : 'px-3 py-1.5';
  return (
    <ToggleGroup
      type="single"
      value={value}
      onValueChange={next => {
        if (typeof next === 'string' && next !== '') onChange(next as T);
      }}
      className="self-start rounded-full bg-secondary p-1"
    >
      {opts.map((o, i) => (
        <ToggleGroupItem
          key={o.value}
          value={o.value}
          isFirst={i === 0}
          isLast={i === opts.length - 1}
          className={`rounded-full border-0 ${itemPad}`}
        >
          <Text
            className={`${txt} font-semibold ${o.value === value ? 'text-foreground' : 'text-muted-foreground'}`}
          >
            {o.label}
          </Text>
        </ToggleGroupItem>
      ))}
    </ToggleGroup>
  );
}
