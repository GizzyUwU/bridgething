import { BridgethingClient } from '@bridgething/client';
import { useEffect, useState } from 'react';

import { Launcher } from './launcher';
import { Welcome } from './welcome';
import { Wizard } from './wizard';

const PHASE_KEY = 'onboarding_phase';

type Phase = 'loading' | 'welcome' | 'wizard' | 'done';

const client = new BridgethingClient();

export default function App() {
  const [phase, setPhase] = useState<Phase>('loading');

  useEffect(() => {
    client.connect();
    let cancelled = false;
    (async () => {
      const r = await client.store.get({ key: PHASE_KEY });
      if (cancelled) return;
      if (!r.ok) {
        setPhase('welcome');
        return;
      }
      const value = r.response.value;
      if (value === 'wizard') setPhase('wizard');
      else if (value === 'done') setPhase('done');
      else setPhase('welcome');
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  if (phase === 'loading') {
    return <div className="flex h-full w-full items-center justify-center bg-bt-charcoal" />;
  }
  if (phase === 'welcome') return <Welcome client={client} />;
  if (phase === 'wizard') return <Wizard client={client} onDone={() => setPhase('done')} />;
  return <Launcher client={client} />;
}
