import type { ReactNode } from 'react';
import { ActivityIndicator, Pressable, Text } from 'react-native';

type Variant = 'primary' | 'secondary' | 'destructive' | 'ghost';

const ROOT: Record<Variant, string> = {
  primary: 'bg-primary',
  secondary: 'bg-secondary',
  destructive: 'bg-destructive',
  ghost: 'bg-transparent',
};

const LABEL: Record<Variant, string> = {
  primary: 'text-primary-foreground',
  secondary: 'text-secondary-foreground',
  destructive: 'text-destructive-foreground',
  ghost: 'text-foreground',
};

export function Button({
  onPress,
  disabled,
  loading,
  variant = 'primary',
  children,
  size = 'md',
  className,
}: {
  onPress?: () => void;
  disabled?: boolean;
  loading?: boolean;
  variant?: Variant;
  size?: 'sm' | 'md';
  children: ReactNode;
  className?: string;
}) {
  const inactive = disabled || loading;
  const padding = size === 'sm' ? 'px-3 py-1.5' : 'px-5 py-2.5';
  return (
    <Pressable
      onPress={onPress}
      disabled={inactive}
      className={`${ROOT[variant]} ${padding} flex-row items-center justify-center rounded-md ${inactive ? 'opacity-50' : ''} ${className ?? ''}`}
    >
      {loading ? (
        <ActivityIndicator size="small" />
      ) : (
        <Text className={`text-sm font-semibold ${LABEL[variant]}`}>
          {children}
        </Text>
      )}
    </Pressable>
  );
}
