import {
  BridgethingSession,
  type SessionEvent,
} from '@bridgething/session-react-native';
import { useEffect, useRef, useSyncExternalStore } from 'react';

let singleton: BridgethingSession | null = null;

/** Process-wide session instance. Lazily constructed on first access so a
 *  SwiftUI hot reload doesn't double-register Nitro callbacks. */
export function getSession(): BridgethingSession {
  if (!singleton) singleton = new BridgethingSession();
  return singleton;
}

/** Subscribe a stable callback to session events. Cleans up on unmount. */
export function useSessionEvents(handler: (event: SessionEvent) => void): void {
  const session = getSession();
  const ref = useRef(handler);
  ref.current = handler;
  useEffect(() => {
    return session.on(event => ref.current(event));
  }, [session]);
}

/** Read a session-derived value reactively; the consumer receives a
 *  fresh snapshot whenever any matching event lands. `selectorEvents`
 *  bounds which event types trigger a re-read; the default is "any". */
export function useSessionValue<T>(
  selector: (session: BridgethingSession) => T,
  selectorEvents?: SessionEvent['type'][],
): T {
  const session = getSession();
  const subscribe = useRef((notify: () => void) => {
    return session.on(event => {
      if (!selectorEvents || selectorEvents.includes(event.type)) {
        notify();
      }
    });
  }).current;
  const get = () => selector(session);
  return useSyncExternalStore(subscribe, get, get);
}
