import { describeError } from '@bridgething/ui/errors';
import { useState } from 'react';
import { View } from 'react-native';

import { openAppSettings } from '../../lib/permissions';
import { usePermissionStatus } from '../../lib/permissions-status';
import { getSession } from '../../lib/session';
import { ListRow } from '../ListRow';
import { RowNote, type RowNotice } from '../RowNote';
import { Switch } from '../ui/switch';

export function DefaultDialerRow() {
  const perm = usePermissionStatus('defaultDialer');
  const [notice, setNotice] = useState<RowNotice | null>(null);

  const claim = async () => {
    setNotice(null);
    try {
      await perm.run(() => getSession().requestDefaultDialer());
    } catch (err) {
      setNotice({ text: describeError(err) });
    }
  };

  const toggle = (next: boolean) => {
    if (!next) {
      setNotice({
        tone: 'warn',
        text: 'android only changes the default phone app from its own settings page.',
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
        icon="Phone"
        iconTint={perm.granted ? 'accent' : 'default'}
        title="phone calls"
        subtitle={
          perm.granted
            ? 'mirroring calls to your car thing'
            : 'tap to make bridgething your phone app'
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
