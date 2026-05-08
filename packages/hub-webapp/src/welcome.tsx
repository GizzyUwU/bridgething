import { type BridgethingClient } from '@bridgething/client';
import { useState } from 'react';

import { Wordmark } from './wordmark';

const PHASE_KEY = 'onboarding_phase';

export function Welcome({ client }: { client: BridgethingClient }) {
  const [rebooting, setRebooting] = useState(false);

  const onReboot = async () => {
    if (rebooting) return;
    setRebooting(true);
    await client.store.put({ key: PHASE_KEY, value: 'wizard' });
    await client.system.reboot();
  };

  return (
    <div className="flex h-full w-full flex-col items-center justify-between bg-bt-charcoal px-8 py-10">
      <div className="flex flex-1 flex-col items-center justify-center gap-10">
        <Wordmark size="md" tagline />
        <div className="flex flex-col items-center gap-2">
          <div className="bt-wordmark text-2xl font-medium text-bt-off-white">installation complete</div>
          <div className="text-center text-sm text-bt-soft-gray">
            first boot prepared the system. give it one reboot to settle in.
          </div>
        </div>
      </div>
      <button
        type="button"
        onClick={onReboot}
        disabled={rebooting}
        className="rounded-full bg-bt-blue px-10 py-3 text-base font-medium text-bt-charcoal transition active:scale-95 disabled:opacity-60">
        {rebooting ? 'rebooting...' : 'reboot now'}
      </button>
    </div>
  );
}
