import type { BridgethingSessionPeer } from '@bridgething/session-react-native';
import { describeError } from '@bridgething/ui/errors';
import { useState } from 'react';
import { View } from 'react-native';

import { getSession, peerDisplayName, useSession } from '../../lib/session';
import { ListRow } from '../ListRow';
import { RowNote, type RowNotice } from '../RowNote';

export function AncsRows() {
  const peers = useSession(s => s.peers);
  const connected = peers.filter(p => p.status === 'connected');

  if (connected.length === 0)
    return (
      <ListRow
        icon="Bluetooth"
        title="notification pairing"
        subtitle="connect your car thing first"
      />
    );

  return (
    <>
      {connected.map(peer => (
        <AncsRow key={peer.id} peer={peer} />
      ))}
    </>
  );
}

function AncsRow({ peer }: { peer: BridgethingSessionPeer }) {
  const status = useSession(s => s.ancsAuthStatus[peer.id] ?? 'unknown');
  const ledger = useSession(s => s.ledger);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<RowNotice | null>(null);

  const pair = async () => {
    if (busy) return;
    setBusy(true);
    setNotice(null);
    try {
      const result = await getSession().enableAncsNotifications(peer.id);
      if (result.kind === 'failed')
        setNotice({
          text: result.message ?? 'try again with your car thing nearby.',
          action: 'retry',
          onAction: () => void pair(),
        });
    } catch (err) {
      setNotice({
        text: describeError(err),
        action: 'retry',
        onAction: () => void pair(),
      });
    } finally {
      setBusy(false);
    }
  };

  return (
    <View>
      <ListRow
        icon="Bluetooth"
        iconTint={status === 'authorized' ? 'accent' : 'default'}
        title={`notification pairing · ${peerDisplayName(peer, ledger)}`}
        subtitle={
          status === 'authorized'
            ? 'paired and authorized'
            : status === 'unauthorized'
              ? 'paired but not authorized · tap to fix'
              : 'tap to pair for notifications and volume'
        }
        loading={busy}
        onPress={status === 'authorized' ? undefined : () => void pair()}
      />
      <RowNote notice={notice} />
    </View>
  );
}
