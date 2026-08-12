import type { ReactNode } from 'react';
import { Text, View } from 'react-native';

import { Icon, type IconName } from './Icon';
import { Press } from './Press';
import { Spinner } from './Spinner';
import { BOX, TEXT } from '../lib/theme';

export type RowTint = 'default' | 'accent' | 'ok' | 'err' | 'warn';

const TINT: Record<RowTint, 'neutral' | 'accent' | 'ok' | 'err' | 'warn'> = {
  default: 'neutral',
  accent: 'accent',
  ok: 'ok',
  err: 'err',
  warn: 'warn',
};

export function ListRow({
  icon,
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
  icon?: IconName;
  iconTint?: RowTint;
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
  const body = (
    <View
      className={`flex-row items-center gap-3 px-4 py-3 ${disabled ? 'opacity-40' : ''}`}
    >
      {icon ? (
        <View
          className="items-center justify-center"
          style={{ width: BOX.sm, height: BOX.sm }}
        >
          <Icon name={icon} tone={TINT[iconTint]} size={18} />
        </View>
      ) : null}
      <View className="min-w-0 flex-1">
        <Text
          className={`font-sans ${destructive ? 'text-err' : 'text-fg'}`}
          style={TEXT.row}
          numberOfLines={1}
        >
          {title}
        </Text>
        {subtitle ? (
          <Text
            className="mt-0.5 font-sans text-muted"
            style={TEXT.hint}
            numberOfLines={2}
          >
            {subtitle}
          </Text>
        ) : null}
      </View>
      {value ? (
        <Text
          className="max-w-[45%] text-right font-mono text-soft"
          style={TEXT.body}
          numberOfLines={1}
        >
          {value}
        </Text>
      ) : null}
      {trailing}
      {loading ? <Spinner /> : null}
      {chevron ? (
        <Text className="font-mono text-dim" style={TEXT.body}>
          ›
        </Text>
      ) : null}
    </View>
  );

  if (!onPress) return body;
  return (
    <Press onPress={onPress} disabled={disabled || loading}>
      {body}
    </Press>
  );
}
