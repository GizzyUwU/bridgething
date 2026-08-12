import { useEffect } from 'react';
import Animated, {
  cancelAnimation,
  useAnimatedStyle,
  useSharedValue,
  withDelay,
  withRepeat,
  withSequence,
  withTiming,
} from 'react-native-reanimated';

import { useAppActive } from '../lib/app-active';
import { TYPE, type Tone } from '../lib/theme';
import { TONE_DOT } from '../lib/tone';

const BLINK_MS = 530;

export function Caret({
  size = TYPE.hero,
  tone = 'accent',
}: {
  size?: number;
  tone?: Tone;
}) {
  const visible = useSharedValue(1);
  const active = useAppActive();

  useEffect(() => {
    if (!active) {
      cancelAnimation(visible);
      visible.value = 1;
      return;
    }
    visible.value = withRepeat(
      withSequence(
        withDelay(BLINK_MS, withTiming(0, { duration: 0 })),
        withDelay(BLINK_MS, withTiming(1, { duration: 0 })),
      ),
      -1,
      false,
    );
    return () => cancelAnimation(visible);
  }, [active, visible]);

  const style = useAnimatedStyle(() => ({ opacity: visible.value }));

  return (
    <Animated.View
      className={TONE_DOT[tone]}
      style={[
        { width: Math.round(size * 0.55), height: Math.round(size * 1.1) },
        style,
      ]}
    />
  );
}
