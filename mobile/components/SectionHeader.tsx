import { RefreshCw } from 'lucide-react-native';
import type { ReactNode } from 'react';
import { ActivityIndicator, Text, View } from 'react-native';

import { Press } from './Press';

export function SectionHeader({
  title,
  hint,
  action,
  onActionPress,
  actionPending = false,
}: {
  title: string;
  hint?: string;
  action?: string;
  onActionPress?: () => void;
  actionPending?: boolean;
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
        <Press
          onPress={onActionPress}
          disabled={actionPending}
          scaleTo={0.9}
          hitSlop={8}
        >
          <View className="-mr-1 flex-row items-center gap-1.5 px-2 py-1">
            {actionPending ? (
              <ActivityIndicator size="small" />
            ) : (
              <RefreshCw size={15} color="hsl(215 14% 45%)" strokeWidth={2.4} />
            )}
            <Text className="text-[13px] font-semibold text-muted-foreground">
              {action}
            </Text>
          </View>
        </Press>
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
