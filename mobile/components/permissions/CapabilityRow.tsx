import { CAPABILITIES, type CapabilityKey } from '../../lib/capabilities';
import { ListRow } from '../ListRow';
import { Switch } from '../ui/switch';

export function CapabilityRow({
  capability,
  value,
  onChange,
  subtitle,
  disabled = false,
  loading = false,
  onPress,
}: {
  capability: CapabilityKey;
  value: boolean;
  onChange: (next: boolean) => void;
  subtitle?: string;
  disabled?: boolean;
  loading?: boolean;
  onPress?: () => void;
}) {
  const copy = CAPABILITIES[capability];

  return (
    <ListRow
      icon={copy.icon}
      iconTint={value ? 'accent' : 'default'}
      title={copy.title}
      subtitle={subtitle ?? copy.subtitle}
      loading={loading}
      onPress={onPress}
      trailing={
        <Switch
          value={value}
          onValueChange={onChange}
          disabled={disabled || loading}
        />
      }
    />
  );
}
