import type { ComponentChildren, VNode } from 'preact';

import { cx } from '../cx.ts';

export type RowTint = 'default' | 'accent' | 'ok' | 'err' | 'warn';

const TINT: Record<RowTint, string> = {
  default: 'text-soft',
  accent: 'text-accent',
  ok: 'text-ok',
  err: 'text-err',
  warn: 'text-warn',
};

export function ListRow({
  icon,
  iconTint = 'default',
  title,
  subtitle,
  value,
  trailing,
  chevron = false,
  destructive = false,
  disabled = false,
  onClick,
}: {
  icon?: ComponentChildren;
  iconTint?: RowTint;
  title: ComponentChildren;
  subtitle?: ComponentChildren;
  value?: ComponentChildren;
  trailing?: ComponentChildren;
  chevron?: boolean;
  destructive?: boolean;
  disabled?: boolean;
  onClick?: () => void;
}): VNode {
  const interactive = Boolean(onClick) && !disabled;

  const body = (
    <>
      {icon ? <span class={cx('flex size-8 shrink-0 items-center justify-center', TINT[iconTint])}>{icon}</span> : null}
      <span class="flex min-w-0 flex-1 flex-col gap-0.5 text-left">
        <span class={cx('truncate text-row', destructive ? 'text-err' : 'text-off-white')}>{title}</span>
        {subtitle ? <span class="text-hint truncate text-muted">{subtitle}</span> : null}
      </span>
      {value ? <span class="text-body shrink-0 font-mono text-soft">{value}</span> : null}
      {trailing}
      {chevron ? (
        <span aria-hidden="true" class="text-dim shrink-0 font-mono text-body">
          &rsaquo;
        </span>
      ) : null}
    </>
  );

  const shape = cx(
    'flex w-full items-center gap-3 px-4 py-3 text-left',
    interactive &&
      'hover:bg-neutral-soft focus-visible:outline-2 focus-visible:outline-accent focus-visible:-outline-offset-2 transition-colors',
    disabled && 'opacity-40',
  );

  if (!interactive) return <div class={shape}>{body}</div>;

  return (
    <button type="button" class={shape} onClick={onClick} disabled={disabled}>
      {body}
    </button>
  );
}
