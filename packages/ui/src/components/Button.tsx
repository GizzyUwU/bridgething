import type { ComponentChildren, VNode } from 'preact';

import { cx } from '../cx.ts';
import { Spinner } from './Spinner.tsx';

export type ButtonVariant = 'primary' | 'secondary' | 'destructive' | 'ghost';
export type ButtonSize = 'sm' | 'md' | 'lg';

const VARIANT: Record<ButtonVariant, string> = {
  primary: 'btn-primary',
  secondary: '',
  destructive: 'btn-destructive',
  ghost: 'btn-ghost',
};

const SIZE: Record<ButtonSize, string> = {
  sm: 'btn-sm',
  md: '',
  lg: 'btn-lg',
};

export function Button({
  variant = 'secondary',
  size = 'md',
  icon,
  loading = false,
  disabled = false,
  full = false,
  type = 'button',
  class: className,
  onClick,
  children,
}: {
  variant?: ButtonVariant;
  size?: ButtonSize;
  icon?: ComponentChildren;
  loading?: boolean;
  disabled?: boolean;
  full?: boolean;
  type?: 'button' | 'submit' | 'reset';
  class?: string;
  onClick?: () => void;
  children: ComponentChildren;
}): VNode {
  return (
    <button
      type={type}
      class={cx('btn justify-center', VARIANT[variant], SIZE[size], full && 'w-full', className)}
      disabled={disabled || loading}
      aria-busy={loading || undefined}
      onClick={onClick}>
      {loading ? <Spinner /> : icon ? <span class="flex shrink-0 items-center">{icon}</span> : null}
      {children}
    </button>
  );
}
