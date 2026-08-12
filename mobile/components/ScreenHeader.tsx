import type { ReactNode } from 'react';
import { Text, View } from 'react-native';

import { SPACE, TEXT } from '../lib/theme';

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
    <View
      className="flex-row items-end justify-between gap-3"
      style={{ marginBottom: SPACE.screenHeader }}
    >
      <View className="flex-1">
        {eyebrow ? (
          <Text
            className="mb-1.5 font-mono uppercase text-accent"
            style={TEXT.eyebrow}
          >
            {eyebrow}
          </Text>
        ) : null}
        <Text className="font-display text-fg" style={TEXT.screenTitle}>
          {title}
        </Text>
        {subtitle ? (
          <Text className="mt-2 font-sans text-muted" style={TEXT.body}>
            {subtitle}
          </Text>
        ) : null}
      </View>
      {trailing ? <View className="pb-1">{trailing}</View> : null}
    </View>
  );
}
