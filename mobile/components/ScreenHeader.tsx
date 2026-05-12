import type { ReactNode } from 'react';
import { Text, View } from 'react-native';

/**
 * Large-title screen header used inside the body of a screen (the nav
 * header sits separate). Title sits left-aligned with optional eyebrow
 * label above and a subtitle below. Trailing slot for an action.
 */
export function ScreenHeader({
  eyebrow,
  title,
  subtitle,
  trailing,
}: {
  eyebrow?: string;
  title: string;
  subtitle?: string;
  trailing?: ReactNode;
}) {
  return (
    <View className="mb-6 mt-1 flex-row items-end justify-between gap-3">
      <View className="flex-1">
        {eyebrow ? (
          <Text className="mb-1 text-[11px] font-semibold uppercase tracking-[0.18em] text-primary">
            {eyebrow}
          </Text>
        ) : null}
        <Text
          className="text-foreground"
          style={{
            fontFamily: 'Outfit-Medium',
            fontSize: 34,
            lineHeight: 38,
            letterSpacing: -1.0,
          }}
        >
          {title}
        </Text>
        {subtitle ? (
          <Text className="mt-1 text-[15px] leading-[22px] text-muted-foreground">
            {subtitle}
          </Text>
        ) : null}
      </View>
      {trailing ? <View className="pb-1">{trailing}</View> : null}
    </View>
  );
}
