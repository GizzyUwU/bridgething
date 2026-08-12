import type { ReactNode } from 'react';
import { Text, View } from 'react-native';

import { Icon, type IconName } from './Icon';
import { Press } from './Press';
import { Spinner } from './Spinner';
import { TYPE, usePalette, type Palette } from '../lib/theme';

export type ButtonVariant = 'primary' | 'secondary' | 'destructive' | 'ghost';
export type ButtonSize = 'sm' | 'md' | 'lg';

const SHAPE: Record<ButtonVariant, string> = {
  primary: 'border-accent bg-accent',
  secondary: 'border-edge',
  destructive: 'border-err bg-err',
  ghost: 'border-transparent',
};

const LABEL: Record<ButtonVariant, string> = {
  primary: 'text-screen',
  secondary: 'text-near',
  destructive: 'text-screen',
  ghost: 'text-soft',
};

const PAD: Record<ButtonSize, string> = {
  sm: 'px-3.5 py-1.5',
  md: 'px-5 py-2.5',
  lg: 'px-6 py-3',
};

const LABEL_SIZE: Record<ButtonSize, number> = {
  sm: TYPE.hint,
  md: TYPE.body,
  lg: TYPE.rowLg,
};

function labelColor(
  palette: Palette,
  variant: ButtonVariant,
  inactive: boolean,
): string {
  if (inactive) return palette.dim;
  if (variant === 'primary' || variant === 'destructive') return palette.screen;
  return variant === 'ghost' ? palette.soft : palette.near;
}

export function Button({
  onPress,
  disabled,
  loading,
  variant = 'primary',
  size = 'md',
  icon,
  full = true,
  className,
  children,
}: {
  onPress?: () => void;
  disabled?: boolean;
  loading?: boolean;
  variant?: ButtonVariant;
  size?: ButtonSize;
  icon?: IconName;
  full?: boolean;
  className?: string;
  children: ReactNode;
}) {
  const palette = usePalette();
  const inactive = Boolean(disabled || loading);
  const tint = labelColor(palette, variant, inactive);

  return (
    <Press
      onPress={onPress}
      disabled={inactive}
      className={`${full ? 'self-stretch' : 'self-start'} ${className ?? ''}`}
    >
      <View
        className={`flex-row items-center justify-center gap-2 border ${PAD[size]} ${
          inactive ? 'border-dashed border-rule' : SHAPE[variant]
        }`}
      >
        {loading ? (
          <Spinner color={tint} />
        ) : icon ? (
          <Icon name={icon} color={tint} size={LABEL_SIZE[size] + 3} />
        ) : null}
        <Text
          className={`font-mono ${inactive ? 'text-dim' : LABEL[variant]}`}
          style={{ fontSize: LABEL_SIZE[size] }}
        >
          {children}
        </Text>
      </View>
    </Press>
  );
}
