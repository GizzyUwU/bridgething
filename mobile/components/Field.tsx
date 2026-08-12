import { useState } from 'react';
import { Text, TextInput, type TextInputProps, View } from 'react-native';

import { Icon, type IconName } from './Icon';
import { Press } from './Press';
import { TEXT, usePalette } from '../lib/theme';

export function Field({
  label,
  hint,
  icon,
  value,
  onChangeText,
  onCommit,
  clearable,
  ...rest
}: {
  label?: string;
  hint?: string;
  icon?: IconName;
  value: string;
  onChangeText: (next: string) => void;
  onCommit?: (value: string) => void;
  clearable?: boolean;
} & Omit<TextInputProps, 'value' | 'onChangeText'>) {
  const [focused, setFocused] = useState(false);
  const palette = usePalette();

  return (
    <View>
      {label ? (
        <Text
          className="mb-1.5 font-mono uppercase text-muted"
          style={TEXT.eyebrow}
        >
          {label}
        </Text>
      ) : null}
      <View
        className={`flex-row items-center gap-2 border bg-screen px-3 ${
          focused ? 'border-accent' : 'border-rule-strong'
        }`}
      >
        {icon ? <Icon name={icon} size={18} color={palette.dim} /> : null}
        <TextInput
          {...rest}
          value={value}
          onChangeText={onChangeText}
          onFocus={e => {
            setFocused(true);
            rest.onFocus?.(e);
          }}
          onBlur={e => {
            setFocused(false);
            rest.onBlur?.(e);
          }}
          onEndEditing={e => {
            onCommit?.(value);
            rest.onEndEditing?.(e);
          }}
          placeholderTextColor={palette.dim}
          className="flex-1 py-2.5 font-sans text-fg"
          style={TEXT.row}
        />
        {clearable && value.length > 0 ? (
          <Press onPress={() => onChangeText('')} hitSlop={10}>
            <Text className="px-1 font-mono text-dim" style={TEXT.body}>
              ×
            </Text>
          </Press>
        ) : null}
      </View>
      {hint ? (
        <Text className="mt-1.5 font-sans text-muted" style={TEXT.hint}>
          {hint}
        </Text>
      ) : null}
    </View>
  );
}
