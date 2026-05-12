import type { LucideIcon } from 'lucide-react-native';
import { View } from 'react-native';

const TINT = {
  primary: { bg: 'bg-primary-soft', stroke: 'hsl(199 100% 44%)' },
  neutral: { bg: 'bg-secondary', stroke: 'hsl(215 14% 38%)' },
  destructive: { bg: 'bg-destructive-soft', stroke: 'hsl(0 72% 50%)' },
  success: { bg: 'bg-success-soft', stroke: 'hsl(152 60% 38%)' },
} as const;

type Tint = keyof typeof TINT;

/**
 * Decorative icon container — used at the top of detail screens, in
 * onboarding step cards, and other places where a row tile would be
 * too compact.
 */
export function IconBadge({
  icon: Icon,
  tint = 'primary',
  size = 56,
}: {
  icon: LucideIcon;
  tint?: Tint;
  size?: number;
}) {
  const t = TINT[tint];
  return (
    <View
      className={`items-center justify-center rounded-2xl ${t.bg}`}
      style={{ width: size, height: size }}
    >
      <Icon size={Math.round(size * 0.5)} color={t.stroke} strokeWidth={2.1} />
    </View>
  );
}
