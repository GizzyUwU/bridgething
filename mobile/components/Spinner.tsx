import { ActivityIndicator } from 'react-native';

import { type Tone, useToneColor } from '../lib/theme';

export function Spinner({
  tone = 'neutral',
  color,
  size = 'small',
}: {
  tone?: Tone;
  color?: string;
  size?: 'small' | 'large';
}) {
  const toneColor = useToneColor(tone);
  return <ActivityIndicator size={size} color={color ?? toneColor} />;
}
