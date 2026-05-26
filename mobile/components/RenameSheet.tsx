import { useEffect, useState } from 'react';
import { Text, TextInput, View } from 'react-native';

import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from './ui/alert-dialog';
import { Button } from './Button';

/**
 * Rename prompt built on RNR's `<AlertDialog>` (modal portal + focus trap).
 * Submitting an empty string yields `null` (clears the nickname).
 */
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
    <AlertDialog open={visible} onOpenChange={open => !open && onClose()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            <Text style={{ letterSpacing: -0.4 }}>{title}</Text>
          </AlertDialogTitle>
          {message ? (
            <AlertDialogDescription>{message}</AlertDialogDescription>
          ) : null}
        </AlertDialogHeader>
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
          className="rounded-xl bg-surface-subtle px-4 py-3 text-[16px] text-foreground"
        />
        <AlertDialogFooter>
          <View className="flex-1">
            <Button onPress={onClose} variant="ghost" size="md">
              cancel
            </Button>
          </View>
          <View className="flex-1">
            <Button onPress={submit} size="md">
              save
            </Button>
          </View>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
