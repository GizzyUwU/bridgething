import { useState } from 'react';
import { Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { Press } from './Press';
import { StatusSheet } from './StatusSheet';
import { useConditions } from '../lib/status';
import { TEXT } from '../lib/theme';
import { TONE_DOT, TONE_TEXT } from '../lib/tone';

const STRIP_HEIGHT = 28;

const NATIVE_TAB_BAR_HEIGHT = 49;

export function StatusLine({ floating = false }: { floating?: boolean }) {
  const list = useConditions();
  const [open, setOpen] = useState(false);
  const insets = useSafeAreaInsets();

  if (list.length === 0) return null;

  const top = list[0];
  const label = `${top.label}${list.length > 1 ? ` · ${list.length} issues` : ''}`;

  const strip = (
    <View
      className="flex-row items-center gap-2 border-t border-rule bg-screen px-4"
      style={{ height: STRIP_HEIGHT }}
    >
      <View className={`h-1.5 w-1.5 ${TONE_DOT[top.tone]}`} />
      <Text
        className={`flex-1 font-mono uppercase ${TONE_TEXT[top.tone]}`}
        style={TEXT.eyebrow}
        numberOfLines={1}
      >
        {label}
      </Text>
    </View>
  );

  return (
    <>
      <Press
        onPress={() => setOpen(true)}
        style={
          floating
            ? {
                position: 'absolute',
                left: 0,
                right: 0,
                bottom: insets.bottom + NATIVE_TAB_BAR_HEIGHT,
              }
            : undefined
        }
      >
        {strip}
      </Press>
      <StatusSheet
        visible={open}
        conditions={list}
        onClose={() => setOpen(false)}
      />
    </>
  );
}
