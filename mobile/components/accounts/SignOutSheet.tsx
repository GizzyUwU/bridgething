import { ConfirmSheet } from '../ConfirmSheet';
import type { Accounts } from './useAccounts';

export function SignOutSheet({ accounts }: { accounts: Accounts }) {
  const provider = accounts.leaving;

  return (
    <ConfirmSheet
      visible={provider != null}
      title="sign out?"
      body={
        provider
          ? `${provider.displayName} will be signed out on this phone.`
          : undefined
      }
      confirmLabel="sign out"
      destructive
      busy={accounts.leaveBusy}
      onConfirm={accounts.confirmSignOut}
      onClose={accounts.dismissSignOut}
    />
  );
}
