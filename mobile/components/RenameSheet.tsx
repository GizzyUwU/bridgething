import { useEffect, useState } from 'react';
import { Text, TextInput, View } from 'react-native';

import { Button } from './Button';
import { Sheet } from './Sheet';

export function RenameSheet({
  visible,
  title,
  message,
  initialValue,
  placeholder,
  onSubmit,
  onClose,
}: {
  visible: boolean;
  title: string;
  message?: string;
  initialValue?: string;
  placeholder?: string;
  onSubmit: (value: string | null) => void;
  onClose: () => void;
}) {
  const [draft, setDraft] = useState(initialValue ?? '');

  useEffect(() => {
    if (visible) setDraft(initialValue ?? '');
  }, [visible, initialValue]);

  const submit = () => {
    const trimmed = draft.trim();
    onSubmit(trimmed === '' ? null : trimmed);
    onClose();
  };

  return (
    <Sheet visible={visible} onClose={onClose}>
      <View className="gap-1">
        <Text
          className="text-[19px] font-bold text-foreground"
          style={{ letterSpacing: -0.4 }}
        >
          {title}
        </Text>
        {message ? (
          <Text className="text-[13px] leading-[18px] text-muted-foreground">
            {message}
          </Text>
        ) : null}
      </View>
      <TextInput
        value={draft}
        onChangeText={setDraft}
        placeholder={placeholder}
        placeholderTextColor="hsl(215 14% 55%)"
        autoFocus
        returnKeyType="done"
        onSubmitEditing={submit}
        autoCapitalize="words"
        autoCorrect={false}
        className="rounded-2xl border border-border bg-surface px-4 py-3.5 text-[16px] text-foreground"
      />
      <View className="flex-row gap-3">
        <View className="flex-1">
          <Button onPress={onClose} variant="secondary" size="lg">
            cancel
          </Button>
        </View>
        <View className="flex-1">
          <Button onPress={submit} size="lg">
            save
          </Button>
        </View>
      </View>
    </Sheet>
  );
}
