import { useEffect, useState } from 'react';
import { View, type LayoutChangeEvent } from 'react-native';
import Animated, {
  Easing,
  cancelAnimation,
  useAnimatedStyle,
  useSharedValue,
  withRepeat,
  withTiming,
} from 'react-native-reanimated';

import { useAppActive } from '../lib/app-active';
import { TONE_DOT } from '../lib/tone';
import type { Tone } from '../lib/theme';

const TRACK_HEIGHT = 4;
const SWEEP_MS = 1400;

export function Progress({
  percent,
  tone = 'accent',
  className,
}: {
  percent: number | null;
  tone?: Tone;
  className?: string;
}) {
  if (percent === null) return <Sweep tone={tone} className={className} />;

  return (
    <View
      className={`w-full overflow-hidden bg-neutral-soft ${className ?? ''}`}
      style={{ height: TRACK_HEIGHT }}
    >
      <View
        className={`h-full ${TONE_DOT[tone]}`}
        style={{ width: `${Math.max(0, Math.min(100, percent))}%` }}
      />
    </View>
  );
}

function Sweep({ tone, className }: { tone: Tone; className?: string }) {
  const [width, setWidth] = useState(0);
  const offset = useSharedValue(0);
  const active = useAppActive();

  useEffect(() => {
    if (!active || width === 0) {
      cancelAnimation(offset);
      offset.value = 0;
      return;
    }
    offset.value = withRepeat(
      withTiming(1, { duration: SWEEP_MS, easing: Easing.inOut(Easing.ease) }),
      -1,
      false,
    );
    return () => cancelAnimation(offset);
  }, [active, width, offset]);

  const style = useAnimatedStyle(() => ({
    transform: [{ translateX: -width / 3 + offset.value * width * 1.34 }],
  }));

  const onLayout = (event: LayoutChangeEvent) =>
    setWidth(event.nativeEvent.layout.width);

  return (
    <View
      className={`w-full overflow-hidden bg-neutral-soft ${className ?? ''}`}
      style={{ height: TRACK_HEIGHT }}
      onLayout={onLayout}
    >
      <Animated.View
        className={`h-full ${TONE_DOT[tone]}`}
        style={[{ width: width / 3 }, style]}
      />
    </View>
  );
}
