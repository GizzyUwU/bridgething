import { forwardRef, type ReactNode } from 'react';
import {
  Pressable,
  type PressableProps,
  type StyleProp,
  type ViewStyle,
} from 'react-native';
import Animated, {
  useAnimatedStyle,
  useSharedValue,
  withSpring,
  withTiming,
} from 'react-native-reanimated';

const AnimatedPressable = Animated.createAnimatedComponent(Pressable);

type Props = Omit<PressableProps, 'children' | 'style'> & {
  children: ReactNode;
  scaleTo?: number;
  fade?: boolean;
  className?: string;
  style?: StyleProp<ViewStyle>;
};

/**
 * Pressable that springs slightly inward + dims while held. The base
 * affordance for every interactive surface in the app — wrap `Press` /
 * `PressFlat` around tiles, list rows, action buttons. Disabled mode
 * skips the animation entirely so you don't get a "tap-but-stuck" feel.
 */
export const Press = forwardRef<typeof AnimatedPressable, Props>(
  function Press(
    { children, scaleTo = 0.97, fade = true, disabled, style, ...rest },
    _ref,
  ) {
    const scale = useSharedValue(1);
    const opacity = useSharedValue(1);

    const animated = useAnimatedStyle(() => ({
      transform: [{ scale: scale.value }],
      opacity: opacity.value,
    }));

    const onPressIn = (e: Parameters<NonNullable<PressableProps['onPressIn']>>[0]) => {
      if (!disabled) {
        scale.value = withSpring(scaleTo, { mass: 0.4, stiffness: 320, damping: 22 });
        if (fade) opacity.value = withTiming(0.78, { duration: 90 });
      }
      rest.onPressIn?.(e);
    };
    const onPressOut = (e: Parameters<NonNullable<PressableProps['onPressOut']>>[0]) => {
      scale.value = withSpring(1, { mass: 0.4, stiffness: 320, damping: 22 });
      if (fade) opacity.value = withTiming(1, { duration: 140 });
      rest.onPressOut?.(e);
    };

    return (
      <AnimatedPressable
        {...rest}
        disabled={disabled}
        onPressIn={onPressIn}
        onPressOut={onPressOut}
        style={[animated, style]}
      >
        {children}
      </AnimatedPressable>
    );
  },
);
