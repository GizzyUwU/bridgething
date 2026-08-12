import type { BridgethingProviderInfo } from '@bridgething/session-react-native';
import { describeError } from '@bridgething/ui/errors';
import { useState } from 'react';

import { getSession, useSession } from '../../lib/session';

export type Accounts = {
  providers: BridgethingProviderInfo[];
  offered: BridgethingProviderInfo[];
  connected: BridgethingProviderInfo[];
  awaitingAuth: BridgethingProviderInfo[];
  priority: string[];
  libraryProvider: string | null;
  signedIn: boolean;
  busyId: string | null;
  leaving: BridgethingProviderInfo | null;
  leaveBusy: boolean;
  failure: string | null;
  signIn: (id: string) => void;
  cancelAuth: (id: string) => void;
  askSignOut: (provider: BridgethingProviderInfo) => void;
  dismissSignOut: () => void;
  confirmSignOut: () => void;
  promote: (id: string) => void;
};

export function useAccounts(): Accounts {
  const session = getSession();

  const providers = useSession(s => s.providers);
  const priority = useSession(s => s.providerPriority);
  const libraryProvider = useSession(s => s.libraryProvider);

  const [busyId, setBusyId] = useState<string | null>(null);
  const [leaving, setLeaving] = useState<BridgethingProviderInfo | null>(null);
  const [leaveBusy, setLeaveBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);

  const signIn = (id: string) => {
    if (busyId) return;
    setBusyId(id);
    setFailure(null);
    void session
      .connectProvider(id)
      .catch(() => {})
      .finally(() => setBusyId(null));
  };

  const cancelAuth = (id: string) => {
    void session.cancelAuth(id);
    setBusyId(null);
  };

  const confirmSignOut = () => {
    const provider = leaving;
    if (!provider || leaveBusy) return;
    setLeaveBusy(true);
    setFailure(null);
    void session
      .disconnectProvider(provider.id)
      .then(() => setLeaving(null))
      .catch((err: unknown) => setFailure(describeError(err)))
      .finally(() => setLeaveBusy(false));
  };

  const promote = (id: string) => {
    const rest = providers.map(p => p.id).filter(x => x !== id);
    setFailure(null);
    void session
      .setProviderPriority([id, ...rest])
      .catch((err: unknown) => setFailure(describeError(err)));
  };

  return {
    providers,
    offered: providers.filter(p => p.available),
    connected: providers.filter(p => p.connected),
    awaitingAuth: providers.filter(
      p => p.authState.kind === 'pending' || p.authState.kind === 'failed',
    ),
    priority,
    libraryProvider,
    signedIn: providers.some(p => p.connected),
    busyId,
    leaving,
    leaveBusy,
    failure,
    signIn,
    cancelAuth,
    askSignOut: setLeaving,
    dismissSignOut: () => setLeaving(null),
    confirmSignOut,
    promote,
  };
}
