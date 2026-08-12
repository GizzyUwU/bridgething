import { BottomTabBarHeightContext } from '@react-navigation/bottom-tabs';
import { useContext, type ReactNode } from 'react';
import {
  ScrollView,
  type ScrollViewProps,
  View,
  type ViewStyle,
} from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { SPACE } from '../lib/theme';

export function useScreenPadding(): ViewStyle {
  const insets = useSafeAreaInsets();
  const inTabs = useContext(BottomTabBarHeightContext) != null;
  return {
    paddingHorizontal: SPACE.gutter,
    paddingTop: SPACE.headingGap,
    paddingBottom: (inTabs ? 0 : insets.bottom) + SPACE.section,
  };
}

export function ScrollScreen({
  children,
  contentContainerStyle,
  ...scrollProps
}: { children: ReactNode } & ScrollViewProps) {
  const padding = useScreenPadding();
  return (
    <View className="flex-1 bg-bg">
      <ScrollView
        contentInsetAdjustmentBehavior="automatic"
        showsVerticalScrollIndicator={false}
        keyboardShouldPersistTaps="handled"
        {...scrollProps}
        contentContainerStyle={[padding, contentContainerStyle]}
      >
        {children}
      </ScrollView>
    </View>
  );
}
