import type { ReactNode } from 'react';
import { useEffect, useState } from 'react';
import { Modal, Pressable, StyleSheet, View } from 'react-native';
import Animated, {
  runOnJS,
  useAnimatedKeyboard,
  useAnimatedStyle,
  useSharedValue,
  withTiming,
} from 'react-native-reanimated';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

export function Sheet({
  visible,
  onClose,
  children,
}: {
  visible: boolean;
  onClose: () => void;
  children: ReactNode;
}) {
  const insets = useSafeAreaInsets();
  const [render, setRender] = useState(visible);
  const progress = useSharedValue(visible ? 1 : 0);
  const keyboard = useAnimatedKeyboard();

  useEffect(() => {
    if (visible) {
      setRender(true);
      progress.value = withTiming(1, { duration: 200 });
    } else {
      progress.value = withTiming(0, { duration: 150 }, finished => {
        if (finished) runOnJS(setRender)(false);
      });
    }
  }, [visible, progress]);

  const backdropStyle = useAnimatedStyle(() => ({ opacity: progress.value }));
  const cardStyle = useAnimatedStyle(() => ({
    opacity: progress.value,
    transform: [{ translateY: (1 - progress.value) * 18 }],
  }));
  const frameStyle = useAnimatedStyle(() => ({
    paddingTop: insets.top + 16,
    paddingBottom: keyboard.height.value + 16,
  }));

  if (!render) return null;

  return (
    <Modal
      transparent
      visible
      animationType="none"
      statusBarTranslucent
      onRequestClose={onClose}
    >
      <Animated.View
        className="flex-1 items-center justify-center px-4"
        style={frameStyle}
      >
        <Pressable style={StyleSheet.absoluteFill} onPress={onClose}>
          <Animated.View
            style={[StyleSheet.absoluteFill, styles.scrim, backdropStyle]}
          />
        </Pressable>
        <Animated.View className="w-full max-w-[440px]" style={cardStyle}>
          <View className="gap-4 rounded-3xl border border-border bg-background p-6">
            {children}
          </View>
        </Animated.View>
      </Animated.View>
    </Modal>
  );
}

const styles = StyleSheet.create({
  scrim: { backgroundColor: 'rgba(0,0,0,0.55)' },
});
