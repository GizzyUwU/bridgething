import { forwardRef, useState, type ReactNode } from 'react';
import {
  Pressable,
  type PressableProps,
  type StyleProp,
  type View,
  type ViewStyle,
} from 'react-native';

import { usePalette } from '../lib/theme';

type Props = Omit<PressableProps, 'children' | 'style'> & {
  children: ReactNode;
  className?: string;
  style?: StyleProp<ViewStyle>;
};

export const Press = forwardRef<View, Props>(function Press(
  { children, disabled, style, onPressIn, onPressOut, ...rest },
  ref,
) {
  const palette = usePalette();
  const [pressed, setPressed] = useState(false);

  return (
    <Pressable
      {...rest}
      ref={ref}
      disabled={disabled}
      onPressIn={event => {
        setPressed(true);
        onPressIn?.(event);
      }}
      onPressOut={event => {
        setPressed(false);
        onPressOut?.(event);
      }}
      style={[
        pressed && !disabled
          ? { backgroundColor: palette.neutralSoft }
          : undefined,
        style,
      ]}
    >
      {children}
    </Pressable>
  );
});
