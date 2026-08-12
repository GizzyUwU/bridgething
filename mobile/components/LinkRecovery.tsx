import type { BridgethingSessionPeer } from '@bridgething/session-react-native';
import { describeError } from '@bridgething/ui/errors';
import { useState } from 'react';
import { Text, View } from 'react-native';

import { Button } from './Button';
import { Note } from './Note';
import { getSession } from '../lib/bridge';
import { TEXT } from '../lib/theme';

export function LinkRecovery({ peer }: { peer: BridgethingSessionPeer }) {
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);

  const reconnect = async () => {
    setBusy(true);
    setFailure(null);
    try {
      await getSession().reconnectPeer(peer.id);
    } catch (err) {
      setFailure(describeError(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <View className="gap-3 border border-rule bg-screen p-4">
      <Text className="font-sans text-muted" style={TEXT.hint}>
        it is attached to this phone but the link did not open.
      </Text>
      {peer.linkError ? (
        <Note tone="err">{describeError(peer.linkError)}</Note>
      ) : null}
      {failure ? <Note tone="err">{failure}</Note> : null}
      <Button onPress={reconnect} loading={busy} variant="secondary" size="md">
        try reconnect
      </Button>
    </View>
  );
}
