import { Children, type ReactNode } from 'react';
import { View } from 'react-native';

export function ListGroup({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  const items = Children.toArray(children).filter(c => c != null);
  return (
    <View className={`border border-rule bg-screen ${className ?? ''}`}>
      {items.map((child, idx) => (
        <View key={idx}>
          {idx > 0 ? <View className="h-px bg-rule" /> : null}
          {child}
        </View>
      ))}
    </View>
  );
}
