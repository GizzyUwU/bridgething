import * as Lucide from 'lucide-react-native';
import type { LucideIcon } from 'lucide-react-native';

import { type Tone, useToneColor } from '../lib/theme';

export type IconName = Exclude<
  keyof typeof Lucide,
  'Icon' | 'createLucideIcon'
>;

type IconProps = {
  name: IconName;
  tone?: Tone;
  color?: string;
  size?: number;
};

export function Icon({ name, tone = 'neutral', color, size = 20 }: IconProps) {
  const toneColor = useToneColor(tone);
  const Glyph = Lucide[name] as unknown as LucideIcon;

  return (
    <Glyph
      size={size}
      color={color ?? toneColor}
      strokeWidth={1.7}
      strokeLinecap="square"
      strokeLinejoin="miter"
    />
  );
}
