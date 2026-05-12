import { ChevronRight, type LucideIcon } from 'lucide-react-native';
import { Text, View } from 'react-native';

import { Press } from './Press';

type Tone = 'good' | 'info' | 'warn';

const toneStyles: Record<
  Tone,
  { bg: string; border: string; dot: string; text: string }
> = {
  good: {
    bg: 'bg-success-soft',
    border: 'border-success/30',
    dot: 'bg-success',
    text: 'text-success-foreground',
  },
  info: {
    bg: 'bg-secondary',
    border: 'border-border',
    dot: 'bg-muted-foreground',
    text: 'text-foreground',
  },
  warn: {
    bg: 'bg-warning/12',
    border: 'border-warning/40',
    dot: 'bg-warning',
    text: 'text-foreground',
  },
};

/**
 * Top-of-screen "is everything good?" strip. The whole row is tappable
 * when an action is provided; otherwise it's a passive indicator.
 */
export function StatusStrip({
  tone,
  title,
  subtitle,
  icon: Icon,
  onPress,
}: {
  tone: Tone;
  title: string;
  subtitle?: string;
  icon?: LucideIcon;
  onPress?: () => void;
}) {
  const t = toneStyles[tone];
  const body = (
    <View
      className={`flex-row items-center gap-3 rounded-2xl border px-4 py-3 ${t.bg} ${t.border}`}
    >
      <View className={`h-2 w-2 rounded-full ${t.dot}`} />
      <View className="flex-1">
        <Text
          className={`text-[14px] font-semibold ${t.text}`}
          numberOfLines={1}
        >
          {title}
        </Text>
        {subtitle ? (
          <Text
            className="mt-0.5 text-[12px] text-muted-foreground"
            numberOfLines={1}
          >
            {subtitle}
          </Text>
        ) : null}
      </View>
      {Icon ? (
        <Icon size={16} color="hsl(215 14% 50%)" strokeWidth={2.4} />
      ) : null}
      {onPress ? (
        <ChevronRight size={16} color="hsl(215 14% 60%)" strokeWidth={2.4} />
      ) : null}
    </View>
  );

  if (!onPress) return body;
  return (
    <Press onPress={onPress} fade={false} scaleTo={0.99}>
      {body}
    </Press>
  );
}
