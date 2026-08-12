import {
  CAPABILITIES,
  CAPABILITY_GROUPS,
  CAPABILITY_KEYS,
  capabilitiesIn,
} from '../lib/capabilities';
import {
  INITIAL_PERMISSIONS,
  PERMISSION_KEYS,
  reducePermissions,
} from '../lib/permissions-status';

describe('permission status reducer', () => {
  test('every key starts unread so each row shows the same loading state', () => {
    for (const key of PERMISSION_KEYS)
      expect(INITIAL_PERMISSIONS[key]).toEqual({ state: null, busy: false });
  });

  test('returning to the foreground re-reads without dropping what is known', () => {
    const granted = reducePermissions(INITIAL_PERMISSIONS, {
      kind: 'settled',
      key: 'location',
      state: 'granted',
    });

    const reading = reducePermissions(granted, {
      kind: 'reading',
      keys: PERMISSION_KEYS,
    });

    expect(reading.location).toEqual({ state: 'granted', busy: true });
  });

  test('a grant made in system settings lands on the next read', () => {
    const denied = reducePermissions(INITIAL_PERMISSIONS, {
      kind: 'settled',
      key: 'notificationAccess',
      state: 'denied',
    });
    const reading = reducePermissions(denied, {
      kind: 'reading',
      keys: ['notificationAccess'],
    });
    const settled = reducePermissions(reading, {
      kind: 'settled',
      key: 'notificationAccess',
      state: 'granted',
    });

    expect(settled.notificationAccess).toEqual({
      state: 'granted',
      busy: false,
    });
  });

  test('a revoke made in system settings lands the same way', () => {
    const granted = reducePermissions(INITIAL_PERMISSIONS, {
      kind: 'settled',
      key: 'backgroundLocation',
      state: 'granted',
    });
    const revoked = reducePermissions(granted, {
      kind: 'settled',
      key: 'backgroundLocation',
      state: 'blocked',
    });

    expect(revoked.backgroundLocation).toEqual({
      state: 'blocked',
      busy: false,
    });
  });

  test('a read that throws leaves the last known state standing', () => {
    const granted = reducePermissions(INITIAL_PERMISSIONS, {
      kind: 'settled',
      key: 'defaultDialer',
      state: 'granted',
    });
    const reading = reducePermissions(granted, {
      kind: 'reading',
      keys: ['defaultDialer'],
    });
    const failed = reducePermissions(reading, {
      kind: 'failed',
      key: 'defaultDialer',
    });

    expect(failed.defaultDialer).toEqual({ state: 'granted', busy: false });
  });

  test('a read names only the keys it was asked for', () => {
    const reading = reducePermissions(INITIAL_PERMISSIONS, {
      kind: 'reading',
      keys: ['batteryExemption'],
    });

    expect(reading.batteryExemption.busy).toBe(true);
    expect(reading.location.busy).toBe(false);
  });

  test('a settle that changes nothing keeps the same map', () => {
    const granted = reducePermissions(INITIAL_PERMISSIONS, {
      kind: 'settled',
      key: 'location',
      state: 'granted',
    });

    expect(
      reducePermissions(granted, {
        kind: 'settled',
        key: 'location',
        state: 'granted',
      }),
    ).toBe(granted);
    expect(
      reducePermissions(granted, { kind: 'failed', key: 'location' }),
    ).toBe(granted);
  });
});

describe('capability groups', () => {
  test('every capability the phone can lend is described once', () => {
    expect(Object.keys(CAPABILITIES).sort()).toEqual(
      [...CAPABILITY_KEYS].sort(),
    );
  });

  test('every capability lands in exactly one group', () => {
    const placed = CAPABILITY_GROUPS.flatMap(group => capabilitiesIn(group));

    expect(placed.slice().sort()).toEqual([...CAPABILITY_KEYS].sort());
    expect(new Set(placed).size).toBe(placed.length);
  });

  test('the voice group holds the voice model and nothing else', () => {
    expect(capabilitiesIn('voice')).toEqual(['voiceModel']);
  });
});
