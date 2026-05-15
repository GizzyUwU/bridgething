import { View } from 'react-native';
import Animated, {
  useAnimatedStyle,
  withTiming,
} from 'react-native-reanimated';

/**
 * Onboarding progress dots. The active dot stretches into a pill —
 * cheap and effective progress affordance.
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
  const style = useAnimatedStyle(() => ({
    width: withTiming(active ? 22 : 6, { duration: 240 }),
    opacity: withTiming(active ? 1 : 0.35, { duration: 240 }),
  }));
  return (
    <Animated.View className="h-1.5 rounded-full bg-primary" style={style} />
  );
}
