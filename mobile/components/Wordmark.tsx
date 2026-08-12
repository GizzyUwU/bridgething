import { Text, View } from 'react-native';

import { TYPE } from '../lib/theme';

export type WordmarkSize = 'xs' | 'sm' | 'md' | 'lg' | 'xl';

const SIZE: Record<WordmarkSize, number> = {
  xs: TYPE.body,
  sm: TYPE.rowLg,
  md: TYPE.hero,
  lg: TYPE.screenTitle,
  xl: Math.round(TYPE.screenTitle * 1.9),
};

export function Wordmark({
  size = 'md',
  className,
}: {
  size?: WordmarkSize;
  className?: string;
}) {
  const fontSize = SIZE[size];
  const style = {
    fontSize,
    lineHeight: Math.round(fontSize * 1.1),
    letterSpacing: fontSize * -0.03,
  };

  return (
    <View className={`flex-row items-end ${className ?? ''}`}>
      <Text className="font-display text-fg" style={style}>
        bridge
      </Text>
      <Text className="font-display-light text-fg" style={style}>
        thing
      </Text>
    </View>
  );
}
