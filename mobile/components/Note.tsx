import type { ReactNode } from 'react';
import { Text, View } from 'react-native';

import { Press } from './Press';
import { TEXT, type Tone } from '../lib/theme';
import { TONE_BG, TONE_BORDER, TONE_TEXT } from '../lib/tone';

export function Note({
  tone = 'err',
  title,
  action,
  onAction,
  className,
  children,
}: {
  tone?: Tone;
  title?: string;
  action?: string;
  onAction?: () => void;
  className?: string;
  children: ReactNode;
}) {
  return (
    <View
      className={`border px-3 py-2 ${TONE_BORDER[tone]} ${TONE_BG[tone]} ${className ?? ''}`}
    >
      {title ? (
        <Text
          className={`mb-1 font-mono uppercase ${TONE_TEXT[tone]}`}
          style={TEXT.eyebrow}
          numberOfLines={1}
        >
          {title}
        </Text>
      ) : null}
      <Text className={`font-mono ${TONE_TEXT[tone]}`} style={TEXT.hint}>
        {children}
      </Text>
      {action ? (
        <Press onPress={onAction} className="mt-2 self-start px-1 py-0.5">
          <Text
            className={`font-mono uppercase ${TONE_TEXT[tone]}`}
            style={TEXT.eyebrow}
          >
            {action}
          </Text>
        </Press>
      ) : null}
    </View>
  );
}
