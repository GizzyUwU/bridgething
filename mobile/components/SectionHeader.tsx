import type { ReactNode } from 'react';
import { Text, View } from 'react-native';

import { Press } from './Press';
import { Spinner } from './Spinner';
import { SPACE, TEXT } from '../lib/theme';

export function SectionHeader({
  title,
  hint,
  action,
  onAction,
  pending = false,
}: {
  title: string;
  hint?: string;
  action?: string;
  onAction?: () => void;
  pending?: boolean;
}) {
  return (
    <View
      className="flex-row items-end justify-between gap-3"
      style={{ marginBottom: SPACE.headingGap }}
    >
      <View className="flex-1">
        <Text className="font-mono uppercase text-muted" style={TEXT.eyebrow}>
          {title}
        </Text>
        {hint ? (
          <Text className="mt-0.5 font-sans text-muted" style={TEXT.hint}>
            {hint}
          </Text>
        ) : null}
      </View>
      {action ? (
        <Press onPress={onAction} disabled={pending} hitSlop={8}>
          <View className="flex-row items-center gap-1.5 px-1">
            {pending ? <Spinner /> : null}
            <Text
              className={`font-mono uppercase ${pending ? 'text-dim' : 'text-soft'}`}
              style={TEXT.hint}
            >
              {action}
            </Text>
          </View>
        </Press>
      ) : null}
    </View>
  );
}

export function SectionEmpty({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <View
      className={`border border-rule bg-screen px-4 py-6 ${className ?? ''}`}
    >
      <Text className="text-center font-sans text-muted" style={TEXT.body}>
        {children}
      </Text>
    </View>
  );
}
