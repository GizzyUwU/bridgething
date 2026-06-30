import { cva, type VariantProps } from 'class-variance-authority';
import type { LucideIcon } from 'lucide-react-native';
import type { ReactNode } from 'react';
import { ActivityIndicator, View } from 'react-native';

import { Button as RNRButton } from './ui/button';
import { Text } from './ui/text';

/**
 * App-flavoured Button. Composes RNR's `<Button>` with an optional leading
 * icon, a loading spinner, and the soft glow that primary / destructive
 * variants get. `primary`/`tonal` map to RNR's `default`/`secondary`.
 */
type Variant = 'primary' | 'secondary' | 'destructive' | 'ghost' | 'tonal';
type Size = 'sm' | 'md' | 'lg';

const RNR_VARIANT: Record<
  Variant,
  'default' | 'secondary' | 'destructive' | 'ghost'
> = {
  primary: 'default',
  secondary: 'secondary',
  destructive: 'destructive',
  ghost: 'ghost',
  tonal: 'secondary',
};

const RNR_SIZE: Record<Size, 'default' | 'sm' | 'lg'> = {
  sm: 'sm',
  md: 'default',
  lg: 'lg',
};

// Brand-tinted soft-shadow halo for the high-emphasis variants. The
// rest fall back to RNR's built-in surface shadow.
const haloVariants = cva('w-full', {
  variants: {
    variant: {
      primary: 'shadow-primary/30 shadow-md',
      destructive: 'shadow-destructive/30 shadow-md',
      tonal: 'bg-primary-soft',
      secondary: '',
      ghost: '',
    },
  },
  defaultVariants: { variant: 'primary' },
});

const labelToneVariants = cva('text-[15px] font-semibold', {
  variants: {
    variant: {
      primary: 'text-primary-foreground',
      secondary: 'text-secondary-foreground',
      destructive: 'text-white',
      ghost: 'text-foreground',
      tonal: 'text-primary',
    },
  },
  defaultVariants: { variant: 'primary' },
});

type ButtonProps = {
  onPress?: () => void;
  disabled?: boolean;
  loading?: boolean;
  variant?: Variant;
  size?: Size;
  icon?: LucideIcon;
  children: ReactNode;
  className?: string;
  fullWidth?: boolean;
} & VariantProps<typeof haloVariants>;

export function Button({
  onPress,
  disabled,
  loading,
  variant = 'primary',
  size = 'md',
  icon: Icon,
  children,
  className,
  fullWidth,
}: ButtonProps) {
  const inactive = disabled || loading;
  const widthClass = fullWidth === false ? 'self-start' : 'self-stretch';
  const labelClass = labelToneVariants({ variant });
  const tonalClass = variant === 'tonal' ? 'bg-primary-soft' : '';

  const iconColor =
    variant === 'primary' || variant === 'destructive'
      ? 'white'
      : variant === 'tonal'
        ? 'hsl(199 100% 44%)'
        : 'hsl(210 22% 14%)';

  return (
    <RNRButton
      onPress={onPress}
      disabled={inactive}
      variant={RNR_VARIANT[variant]}
      size={RNR_SIZE[size]}
      className={`${widthClass} ${tonalClass} ${
        variant === 'primary' || variant === 'destructive'
          ? haloVariants({ variant })
          : ''
      } ${className ?? ''}`}
    >
      {loading ? (
        <ActivityIndicator size="small" color={iconColor} />
      ) : (
        <View className="h-full w-full flex-row items-center justify-center gap-2">
          {Icon ? <Icon size={18} color={iconColor} strokeWidth={2.4} /> : null}
          <Text className={labelClass}>{children}</Text>
        </View>
      )}
    </RNRButton>
  );
}
