import { createContext, type ComponentChildren, type VNode } from 'preact';
import { useCallback, useContext, useEffect, useRef, useState } from 'preact/hooks';

import { describeError } from './errors.ts';
import type { DeviceSession, Topic } from './session.ts';

const Ctx = createContext<DeviceSession | null>(null);

export function SessionProvider({ session, children }: { session: DeviceSession; children: ComponentChildren }): VNode {
  return <Ctx.Provider value={session}>{children}</Ctx.Provider>;
}

export function useSession(): DeviceSession {
  const session = useContext(Ctx);
  if (!session) throw new Error('a SessionProvider has to be mounted above anything that pulls');
  return session;
}

export type Query<T> = {
  data: T | undefined;
  error: string | undefined;
  loading: boolean;
  refetch: () => void;
};

export function useQuery<T>(
  topics: Topic[],
  pull: (session: DeviceSession) => Promise<T>,
  deps: unknown[] = [],
): Query<T> {
  const session = useSession();
  const [data, setData] = useState<T | undefined>(undefined);
  const [error, setError] = useState<string | undefined>(undefined);
  const [loading, setLoading] = useState(true);

  const generation = useRef(0);
  const held = useRef(pull);
  held.current = pull;

  const run = useCallback(() => {
    const mine = ++generation.current;
    held
      .current(session)
      .then(value => {
        if (mine !== generation.current) return;
        setData(value);
        setError(undefined);
      })
      .catch((reason: unknown) => {
        if (mine !== generation.current) return;
        setError(describeError(reason));
      })
      .finally(() => {
        if (mine === generation.current) setLoading(false);
      });
  }, [session]);

  useEffect(() => {
    run();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [run, ...deps]);

  useEffect(() => {
    const watched = new Set<Topic>(topics);
    return session.subscribe(event => {
      if (watched.has(event.topic)) run();
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session, run, topics.join(',')]);

  return { data, error, loading, refetch: run };
}
