import type { LucideIcon } from 'lucide-react-native';
import { X } from 'lucide-react-native';
import { useState } from 'react';
import {
  Pressable,
  Text,
  TextInput,
  type TextInputProps,
  View,
} from 'react-native';

export function Field({
  label,
  hint,
  icon: Icon,
  value,
  onChangeText,
  onCommit,
  clearable,
  ...rest
}: {
  label?: string;
  hint?: string;
  icon?: LucideIcon;
  value: string;
  onChangeText: (next: string) => void;
  onCommit?: (value: string) => void;
  clearable?: boolean;
} & Omit<TextInputProps, 'value' | 'onChangeText'>) {
  const [focused, setFocused] = useState(false);
  return (
    <View>
      {label ? (
        <Text className="mb-1.5 text-[12px] font-bold uppercase tracking-[0.16em] text-muted-foreground">
          {label}
        </Text>
      ) : null}
      <View
        className={`flex-row items-center gap-2 rounded-2xl border bg-surface px-3.5 ${
          focused ? 'border-primary' : 'border-border'
        }`}
        style={{
          shadowColor: '#000',
          shadowOpacity: focused ? 0.06 : 0.03,
          shadowRadius: focused ? 10 : 6,
          shadowOffset: { width: 0, height: focused ? 4 : 2 },
        }}
      >
        {Icon ? (
          <Icon size={18} color="hsl(215 14% 50%)" strokeWidth={2.1} />
        ) : null}
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
          placeholderTextColor="hsl(215 14% 55%)"
          className="flex-1 py-3.5 text-[15px] text-foreground"
        />
        {clearable && value.length > 0 ? (
          <Pressable onPress={() => onChangeText('')} hitSlop={10}>
            <View className="h-5 w-5 items-center justify-center rounded-full bg-muted">
              <X size={12} color="hsl(215 14% 38%)" strokeWidth={2.6} />
            </View>
          </Pressable>
        ) : null}
      </View>
      {hint ? (
        <Text className="mt-1.5 text-[12px] text-muted-foreground">{hint}</Text>
      ) : null}
    </View>
  );
}
