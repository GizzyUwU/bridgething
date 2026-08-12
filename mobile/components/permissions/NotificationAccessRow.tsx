import { describeError } from '@bridgething/ui/errors';
import { useState } from 'react';
import { View } from 'react-native';

import { usePermissionStatus } from '../../lib/permissions-status';
import { getSession } from '../../lib/session';
import { CapabilityRow } from './CapabilityRow';
import { ConfirmSheet } from '../ConfirmSheet';
import { RowNote, type RowNotice } from '../RowNote';

export function NotificationAccessRow({
  value,
  onChange,
}: {
  value: boolean;
  onChange: (next: boolean) => void;
}) {
  const perm = usePermissionStatus('notificationAccess');
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [notice, setNotice] = useState<RowNotice | null>(null);

  const openAccess = async (): Promise<boolean> => {
    setNotice(null);
    try {
      await perm.run(() => getSession().requestNotificationAccess());
      return true;
    } catch (err) {
      setNotice({ text: describeError(err) });
      return false;
    }
  };

  const toggle = async (next: boolean) => {
    if (!next) {
      setConfirmOpen(true);
      return;
    }
    if (perm.granted) {
      onChange(true);
      return;
    }
    if (await openAccess()) onChange(true);
  };

  const stop = () => {
    setConfirmOpen(false);
    onChange(false);
    void openAccess();
  };

  return (
    <View>
      <ConfirmSheet
        visible={confirmOpen}
        title="stop forwarding notifications?"
        body="bridgething stops sending them right away. android needs you to take the access away on its own settings page."
        confirmLabel="open settings"
        destructive
        onConfirm={stop}
        onClose={() => setConfirmOpen(false)}
      />
      <CapabilityRow
        capability="notifications"
        value={value && perm.granted}
        onChange={next => void toggle(next)}
        subtitle={
          perm.granted
            ? 'forwarding to your car thing'
            : 'tap to allow notification access'
        }
        loading={!perm.ready}
        disabled={perm.busy}
        onPress={perm.granted ? undefined : () => void openAccess()}
      />
      <RowNote notice={notice} />
    </View>
  );
}
