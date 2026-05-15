import { ChevronRight, type LucideIcon } from 'lucide-react-native';
import type { ReactNode } from 'react';
import { Text, View } from 'react-native';

import { Press } from './Press';

/**
 * Composable inset list row. Always renders:
 *   [icon-tile?] [title + subtitle] [trailing? | chevron?]
 *
 * Use chevron=true for navigation rows, value=string for value-display
 * rows (ip, version), trailing=ReactNode for embedding a Switch / Pill
 * / Button.
 */
export function ListRow({
  icon: Icon,
  iconTint = 'default',
  title,
  subtitle,
  value,
  trailing,
  chevron,
  onPress,
  destructive,
  loading,
  disabled,
}: {
  icon?: LucideIcon;
  iconTint?: 'default' | 'primary' | 'destructive' | 'success' | 'warning';
  title: string;
  subtitle?: string;
  value?: string;
  trailing?: ReactNode;
  chevron?: boolean;
  onPress?: () => void;
  destructive?: boolean;
  loading?: boolean;
  disabled?: boolean;
}) {
  const titleColor = destructive ? 'text-destructive' : 'text-foreground';

  const iconBg: Record<string, string> = {
    default: 'bg-secondary',
    primary: 'bg-primary-soft',
    destructive: 'bg-destructive-soft',
    success: 'bg-success-soft',
    warning: 'bg-warning/15',
  };
  const iconColor: Record<string, string> = {
    default: 'hsl(215 14% 38%)',
    primary: 'hsl(199 100% 44%)',
    destructive: 'hsl(0 72% 50%)',
    success: 'hsl(152 60% 38%)',
    warning: 'hsl(38 92% 45%)',
  };

  const body = (
    <View
      className={`flex-row items-center gap-3 px-4 py-3.5 ${disabled ? 'opacity-50' : ''}`}
    >
      {Icon ? (
        <View
          className={`h-9 w-9 items-center justify-center rounded-xl ${iconBg[iconTint]}`}
        >
          <Icon size={18} color={iconColor[iconTint]} strokeWidth={2.2} />
        </View>
      ) : null}
      <View className="flex-1">
        <Text
          className={`text-[15px] font-semibold ${titleColor}`}
          numberOfLines={1}
        >
          {title}
        </Text>
        {subtitle ? (
          <Text
            className="mt-0.5 text-[12.5px] text-muted-foreground"
            numberOfLines={2}
          >
            {subtitle}
          </Text>
        ) : null}
      </View>
      {value ? (
        <Text
          className="ml-1 max-w-[40%] text-right text-[13px] text-muted-foreground"
          numberOfLines={1}
        >
          {value}
        </Text>
      ) : null}
      {trailing ? <View className="ml-1">{trailing}</View> : null}
      {chevron ? (
        <ChevronRight size={18} color="hsl(215 14% 60%)" strokeWidth={2.2} />
      ) : null}
      {loading ? <View className="h-2 w-2 rounded-full bg-primary" /> : null}
    </View>
  );

  if (!onPress) return body;
  return (
    <Press
      onPress={onPress}
      disabled={disabled || loading}
      fade={false}
      scaleTo={1}
    >
      {body}
    </Press>
  );
}
