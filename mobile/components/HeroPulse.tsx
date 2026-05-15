import { useEffect } from 'react';
import { View } from 'react-native';
import Animated, {
  Easing,
  useAnimatedStyle,
  useSharedValue,
  withDelay,
  withRepeat,
  withTiming,
} from 'react-native-reanimated';

const DURATION = 2400;
const PEAK_OPACITY = 0.5;

/**
 * Animated halo used in the "no device connected" state and during the
 * ANCS pairing step. Two rings travel the same scale/opacity curve a
 * half-cycle apart so coverage is always continuous; opacity hits zero
 * at both ends of the cycle so the wrap-around is invisible.
 */
export function HeroPulse({
  tint = 'primary',
}: {
  tint?: 'primary' | 'muted';
}) {
  const phase1 = useSharedValue(0);
  const phase2 = useSharedValue(0);

  useEffect(() => {
    phase1.value = withRepeat(
      withTiming(1, { duration: DURATION, easing: Easing.linear }),
      -1,
      false,
    );
    phase2.value = withDelay(
      DURATION / 2,
      withRepeat(
        withTiming(1, { duration: DURATION, easing: Easing.linear }),
        -1,
        false,
      ),
    );
  }, [phase1, phase2]);

  const ring1 = useAnimatedStyle(() => ringStyle(phase1.value));
  const ring2 = useAnimatedStyle(() => ringStyle(phase2.value));

  const ringClass = tint === 'primary' ? 'bg-primary' : 'bg-muted-foreground';
  const coreClass = tint === 'primary' ? 'bg-primary' : 'bg-muted-foreground';

  return (
    <View className="h-32 w-32 items-center justify-center">
      <Animated.View
        className={`absolute h-32 w-32 rounded-full ${ringClass}`}
        style={ring1}
      />
      <Animated.View
        className={`absolute h-32 w-32 rounded-full ${ringClass}`}
        style={ring2}
      />
      <View className={`h-12 w-12 rounded-full ${coreClass}`} />
    </View>
  );
}

function ringStyle(phase: number) {
  'worklet';
  // sin(π · phase) gives a smooth 0 → peak → 0 envelope across the
  // cycle, so the wrap from 1 back to 0 is always invisible.
  const opacity = Math.sin(Math.PI * phase) * PEAK_OPACITY;
  const scale = 0.6 + phase * 1.0;
  return {
    opacity,
    transform: [{ scale }],
  };
}
