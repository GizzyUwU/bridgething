import { openAppSettings, requestLocation } from '../../lib/permissions';
import { usePermissionStatus } from '../../lib/permissions-status';
import { CapabilityRow } from './CapabilityRow';

export function LocationRow({
  value,
  onChange,
}: {
  value: boolean;
  onChange: (next: boolean) => void;
}) {
  const perm = usePermissionStatus('location');

  const subtitle = perm.blocked
    ? 'location is turned off for bridgething in system settings'
    : perm.unavailable
      ? 'this phone has no location services'
      : undefined;

  const toggle = async (next: boolean) => {
    if (!next) {
      onChange(false);
      return;
    }
    if (perm.granted) {
      onChange(true);
      return;
    }
    if (perm.blocked || perm.unavailable) return;
    await perm.run(async () => {
      if ((await requestLocation()) === 'granted') onChange(true);
    });
  };

  return (
    <CapabilityRow
      capability="geo"
      value={value && perm.granted}
      onChange={next => void toggle(next)}
      subtitle={subtitle}
      loading={!perm.ready}
      disabled={perm.busy || perm.blocked || perm.unavailable}
      onPress={perm.blocked ? openAppSettings : undefined}
    />
  );
}
