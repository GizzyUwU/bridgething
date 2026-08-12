import { View } from 'react-native';

export function PagerDots({ count, index }: { count: number; index: number }) {
  return (
    <View className="flex-row items-center gap-1.5">
      {Array.from({ length: count }).map((_, i) => (
        <View
          key={i}
          className={`h-1.5 w-1.5 ${i === index ? 'bg-accent' : 'bg-edge'}`}
        />
      ))}
    </View>
  );
}
