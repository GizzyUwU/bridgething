import type { ReactNode } from 'react';
import { Text, View } from 'react-native';

import type { Condition } from '../lib/status';
import { TEXT } from '../lib/theme';
import { TONE_DOT, TONE_TEXT } from '../lib/tone';

export function ConditionList({
  conditions,
  action,
}: {
  conditions: Condition[];
  action?: (condition: Condition) => ReactNode;
}) {
  return (
    <View className="gap-4">
      {conditions.map(condition => (
        <View key={condition.id} className="gap-1.5">
          <View className="flex-row items-center gap-2">
            <View className={`h-1.5 w-1.5 ${TONE_DOT[condition.tone]}`} />
            <Text
              className={`font-mono uppercase ${TONE_TEXT[condition.tone]}`}
              style={TEXT.eyebrow}
              numberOfLines={1}
            >
              {condition.label}
            </Text>
          </View>
          <Text className="font-sans text-muted" style={TEXT.hint}>
            {condition.detail}
          </Text>
          {action?.(condition)}
        </View>
      ))}
    </View>
  );
}
