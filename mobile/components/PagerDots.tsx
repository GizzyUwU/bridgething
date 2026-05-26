import { useEffect } from 'react';
import { View } from 'react-native';
import Animated, {
  useAnimatedStyle,
  useSharedValue,
  withTiming,
} from 'react-native-reanimated';

/**
 * Onboarding progress dots. The active dot stretches into a pill.
 */
export function PagerDots({ count, index }: { count: number; index: number }) {
  return (
    <View className="flex-row items-center gap-1.5">
      {Array.from({ length: count }).map((_, i) => (
        <Dot key={i} active={i === index} />
      ))}
    </View>
  );
}

function Dot({ active }: { active: boolean }) {
  // Initial value matches the target so a remount (e.g. after the OS
  // pair picker dismisses) doesn't animate the dots from their default
  // state back to the correct one.
  const width = useSharedValue(active ? 22 : 6);
  const opacity = useSharedValue(active ? 1 : 0.35);
  useEffect(() => {
    width.value = withTiming(active ? 22 : 6, { duration: 240 });
    opacity.value = withTiming(active ? 1 : 0.35, { duration: 240 });
  }, [active, width, opacity]);
  const style = useAnimatedStyle(() => ({
    width: width.value,
    opacity: opacity.value,
  }));
  return (
    <Animated.View className="h-1.5 rounded-full bg-primary" style={style} />
  );
}
