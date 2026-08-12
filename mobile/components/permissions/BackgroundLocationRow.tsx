import { describeError } from '@bridgething/ui/errors';
import { useState } from 'react';
import { View } from 'react-native';

import {
  LOCATION_PERMISSIONS,
  openAppSettings,
  requestBackgroundLocation,
} from '../../lib/permissions';
import { usePermissionStatus } from '../../lib/permissions-status';
import { getSession } from '../../lib/session';
import { ConfirmSheet } from '../ConfirmSheet';
import { ListRow } from '../ListRow';
import { RowNote, type RowNotice } from '../RowNote';
import { Switch } from '../ui/switch';

export function BackgroundLocationRow({
  locationShared,
}: {
  locationShared: boolean;
}) {
  const perm = usePermissionStatus('backgroundLocation');
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [notice, setNotice] = useState<RowNotice | null>(null);

  const subtitle = !locationShared
    ? 'turn on location sharing first'
    : perm.granted
      ? 'sending fixes even while bridgething is in the background'
      : perm.blocked
        ? 'denied at the system level'
        : 'keep sending fixes while bridgething is in the background';

  const grant = async () => {
    setNotice(null);
    try {
      await perm.run(async () => {
        if ((await requestBackgroundLocation()) === 'blocked')
          setNotice({
            tone: 'warn',
            text: 'android only grants this from its own settings page, under allow all the time.',
            action: 'open settings',
            onAction: openAppSettings,
          });
      });
    } catch (err) {
      setNotice({ text: describeError(err) });
    }
  };

  const revoke = async () => {
    setConfirmOpen(false);
    setNotice(null);
    try {
      await perm.run(async () => {
        const session = getSession();
        if (!(await session.revokeRuntimePermissions(LOCATION_PERMISSIONS))) {
          setNotice({
            tone: 'warn',
            text: 'this android version only takes location back from the settings page.',
            action: 'open settings',
            onAction: openAppSettings,
          });
          return;
        }
        setNotice({ tone: 'accent', text: 'restarting bridgething…' });
        setTimeout(() => void session.killApp().catch(() => {}), 500);
      });
    } catch (err) {
      setNotice({ text: describeError(err) });
    }
  };

  return (
    <View>
      <ConfirmSheet
        visible={confirmOpen}
        title="stop sharing in the background?"
        body="android has to close and reopen bridgething to take location away. your car thing reconnects once it is back."
        confirmLabel="revoke + restart"
        destructive
        onConfirm={() => void revoke()}
        onClose={() => setConfirmOpen(false)}
      />
      <ListRow
        icon="MoonStar"
        iconTint={perm.blocked ? 'err' : perm.granted ? 'accent' : 'default'}
        title="background location"
        subtitle={subtitle}
        loading={!perm.ready}
        onPress={perm.blocked ? openAppSettings : undefined}
        trailing={
          <Switch
            value={perm.granted}
            onValueChange={next => {
              if (next) void grant();
              else setConfirmOpen(true);
            }}
            disabled={!locationShared || !perm.ready || perm.busy}
          />
        }
      />
      <RowNote notice={notice} />
    </View>
  );
}
