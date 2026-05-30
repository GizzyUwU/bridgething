import { Text, View } from 'react-native';

import { Button } from './Button';
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogTitle,
} from './ui/alert-dialog';

export type MenuAction = {
  label: string;
  destructive?: boolean;
  onPress: () => void;
};

export function ActionMenu({
  visible,
  title,
  actions,
  onClose,
}: {
  visible: boolean;
  title?: string;
  actions: MenuAction[];
  onClose: () => void;
}) {
  return (
    <AlertDialog open={visible} onOpenChange={open => !open && onClose()}>
      <AlertDialogContent>
        {title ? (
          <AlertDialogHeader>
            <AlertDialogTitle>
              <Text style={{ letterSpacing: -0.4 }}>{title}</Text>
            </AlertDialogTitle>
          </AlertDialogHeader>
        ) : null}
        <View className="gap-2">
          {actions.map(action => (
            <Button
              key={action.label}
              variant={action.destructive ? 'destructive' : 'tonal'}
              size="md"
              onPress={() => {
                onClose();
                action.onPress();
              }}
            >
              {action.label}
            </Button>
          ))}
          <Button variant="ghost" size="md" onPress={onClose}>
            cancel
          </Button>
        </View>
      </AlertDialogContent>
    </AlertDialog>
  );
}
