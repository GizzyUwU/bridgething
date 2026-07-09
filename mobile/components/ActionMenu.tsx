import { Text, View } from 'react-native';

import { Button } from './Button';
import { Sheet } from './Sheet';

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
    <Sheet visible={visible} onClose={onClose}>
      {title ? (
        <Text
          className="text-[19px] font-bold text-foreground"
          style={{ letterSpacing: -0.4 }}
        >
          {title}
        </Text>
      ) : null}
      <View className="gap-2.5">
        {actions.map(action => (
          <Button
            key={action.label}
            variant={action.destructive ? 'destructive' : 'tonal'}
            size="lg"
            onPress={() => {
              onClose();
              action.onPress();
            }}
          >
            {action.label}
          </Button>
        ))}
        <Button variant="ghost" size="lg" onPress={onClose}>
          cancel
        </Button>
      </View>
    </Sheet>
  );
}
