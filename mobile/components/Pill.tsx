import { Text, View } from 'react-native';

type Tone = 'neutral' | 'primary' | 'success' | 'destructive' | 'warning';

const TONE: Record<Tone, { bg: string; fg: string; dot: string }> = {
  neutral: {
    bg: 'bg-secondary',
    fg: 'text-secondary-foreground',
    dot: 'bg-muted-foreground',
  },
  primary: {
    bg: 'bg-primary-soft',
    fg: 'text-primary',
    dot: 'bg-primary',
  },
  success: {
    bg: 'bg-success-soft',
    fg: 'text-success',
    dot: 'bg-success',
  },
  destructive: {
    bg: 'bg-destructive-soft',
    fg: 'text-destructive',
    dot: 'bg-destructive',
  },
  warning: {
    bg: 'bg-warning/15',
    fg: 'text-warning',
    dot: 'bg-warning',
  },
};

/**
 * Compact status indicator. Optional dot prefix, tone controls all
 * three colors. Used for "connected", "signed in", "version pending"
 * style affordances inline within rows or hero blocks.
 */
export function Pill({
  tone = 'neutral',
  dot = true,
  children,
}: {
  tone?: Tone;
  dot?: boolean;
  children: string;
}) {
  const t = TONE[tone];
  return (
    <View
      className={`flex-row items-center gap-1.5 self-start rounded-full px-2.5 py-1 ${t.bg}`}
    >
      {dot ? <View className={`h-1.5 w-1.5 rounded-full ${t.dot}`} /> : null}
      <Text
        className={`text-[11px] font-semibold uppercase tracking-[0.14em] ${t.fg}`}
      >
        {children}
      </Text>
    </View>
  );
}
