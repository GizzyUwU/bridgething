import type { ReactNode } from 'react';
import { Pressable, Text, View } from 'react-native';

/**
 * Inset section label — small, uppercase, tracked. Optional trailing
 * action button (rendered as a primary-tinted text link).
 */
export function SectionHeader({
  title,
  hint,
  action,
  onActionPress,
}: {
  title: string;
  hint?: string;
  action?: string;
  onActionPress?: () => void;
}) {
  return (
    <View className="mb-2 flex-row items-end justify-between px-1">
      <View className="flex-1">
        <Text className="text-[11px] font-bold uppercase tracking-[0.18em] text-muted-foreground">
          {title}
        </Text>
        {hint ? (
          <Text className="mt-0.5 text-[12px] text-muted-foreground">
            {hint}
          </Text>
        ) : null}
      </View>
      {action ? (
        <Pressable onPress={onActionPress} hitSlop={10}>
          <Text className="text-[12px] font-semibold uppercase tracking-[0.16em] text-primary">
            {action}
          </Text>
        </Pressable>
      ) : null}
    </View>
  );
}

export function SectionEmpty({ children }: { children: ReactNode }) {
  return (
    <View className="rounded-2xl border border-border bg-surface px-4 py-6">
      <Text className="text-center text-[13px] text-muted-foreground">
        {children}
      </Text>
    </View>
  );
}
