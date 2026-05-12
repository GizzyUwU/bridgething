import { Children, type ReactNode } from 'react';
import { View } from 'react-native';

/**
 * iOS-style inset grouped container. Children are list rows (or any
 * surface block); separated by a hairline divider drawn between
 * adjacent children. Lifts off the page with a subtle shadow.
 */
export function ListGroup({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  const items = Children.toArray(children).filter(c => c != null);
  return (
    <View
      className={`overflow-hidden rounded-2xl border border-border bg-surface ${className ?? ''}`}
      style={{
        shadowColor: '#000',
        shadowOpacity: 0.06,
        shadowRadius: 14,
        shadowOffset: { width: 0, height: 6 },
        elevation: 1,
      }}
    >
      {items.map((child, idx) => (
        <View key={idx}>
          {idx > 0 ? <View className="ml-14 h-px bg-border" /> : null}
          {child}
        </View>
      ))}
    </View>
  );
}
