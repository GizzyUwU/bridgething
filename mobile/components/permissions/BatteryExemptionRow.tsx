import { describeError } from '@bridgething/ui/errors';
import { useState } from 'react';
import { View } from 'react-native';

import { openAppSettings } from '../../lib/permissions';
import { usePermissionStatus } from '../../lib/permissions-status';
import { getSession } from '../../lib/session';
import { ListRow } from '../ListRow';
import { RowNote, type RowNotice } from '../RowNote';
import { Switch } from '../ui/switch';

export function BatteryExemptionRow() {
  const perm = usePermissionStatus('batteryExemption');
  const [notice, setNotice] = useState<RowNotice | null>(null);

  const claim = async () => {
    setNotice(null);
    try {
      await perm.run(() => getSession().requestIgnoreBatteryOptimizations());
    } catch (err) {
      setNotice({ text: describeError(err) });
    }
  };

  const toggle = (next: boolean) => {
    if (!next) {
      setNotice({
        tone: 'warn',
        text: 'android only puts bridgething back to sleep from its own battery settings page.',
        action: 'open settings',
        onAction: openAppSettings,
      });
      return;
    }
    void claim();
  };

  return (
    <View>
      <ListRow
        icon="BatteryCharging"
        iconTint={perm.granted ? 'accent' : 'default'}
        title="background connection"
        subtitle={
          perm.granted
            ? 'android leaves the connection alone while you drive'
            : 'tap to keep your car thing connected in the background'
        }
        loading={!perm.ready}
        onPress={perm.granted ? undefined : () => void claim()}
        trailing={
          <Switch
            value={perm.granted}
            onValueChange={toggle}
            disabled={!perm.ready || perm.busy}
          />
        }
      />
      <RowNote notice={notice} />
    </View>
  );
}
