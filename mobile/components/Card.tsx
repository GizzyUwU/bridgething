import type { ReactNode } from 'react';
import { Pressable, View } from 'react-native';

export function Card({
  children,
  onPress,
  className,
}: {
  children: ReactNode;
  onPress?: () => void;
  className?: string;
}) {
  if (onPress) {
    return (
      <Pressable
        onPress={onPress}
        className={`rounded-md bg-card p-3 ${className ?? ''}`}
      >
        {children}
      </Pressable>
    );
  }
  return (
    <View className={`rounded-md bg-card p-3 ${className ?? ''}`}>
      {children}
    </View>
  );
}
