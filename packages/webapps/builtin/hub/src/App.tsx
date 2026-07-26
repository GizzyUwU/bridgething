import { BridgethingClient } from '@bridgething/client';
import { useEffect, useState } from 'react';

import { Launcher } from './launcher';
import { Wizard } from './wizard';

const PHASE_KEY = 'onboarding_phase';

type Phase = 'loading' | 'wizard' | 'done';

const client = new BridgethingClient();

export default function App() {
  const [phase, setPhase] = useState<Phase>('loading');

  useEffect(() => {
    client.connect();
    let cancelled = false;
    (async () => {
      const r = await client.store.get({ key: PHASE_KEY });
      if (cancelled) return;
      const value = r.ok ? r.response.value : null;
      setPhase(value === 'done' ? 'done' : 'wizard');
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  if (phase === 'loading') {
    return <div className="flex h-full w-full items-center justify-center bg-bt-charcoal" />;
  }
  if (phase === 'wizard') return <Wizard client={client} onDone={() => setPhase('done')} />;
  return <Launcher client={client} />;
}
