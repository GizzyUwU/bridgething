import { Platform } from 'react-native';
import Animated from 'react-native-reanimated';

/** Animated.View on native; renders children unwrapped on web where Reanimated entering/exiting aren't supported. */
function NativeOnlyAnimatedView(
  props: React.ComponentProps<typeof Animated.View> &
    React.RefAttributes<typeof Animated.View>,
) {
  if (Platform.OS === 'web') {
    return <>{props.children as React.ReactNode}</>;
  } else {
    return <Animated.View {...props} />;
  }
}

export { NativeOnlyAnimatedView };
