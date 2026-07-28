import {
  BridgethingSession,
  type BridgethingSessionSnapshot,
  type SessionEvent,
} from '@bridgething/session-react-native';

let sessionSingleton: BridgethingSession | null = null;

export function getSession(): BridgethingSession {
  if (!sessionSingleton) sessionSingleton = new BridgethingSession();
  return sessionSingleton;
}

export type SessionDomain = {
  name: string;
  apply(event: SessionEvent): void;
  reconcile(snapshot: BridgethingSessionSnapshot): void;
};

const domains: SessionDomain[] = [];
let wired = false;

export function registerDomain(domain: SessionDomain): void {
  if (domains.some(d => d.name === domain.name)) return;
  domains.push(domain);
}

export function startBridge(): void {
  if (wired) return;
  wired = true;
  getSession().subscribe(event => {
    if (event.type === 'resumed') {
      for (const domain of domains) domain.reconcile(event.snapshot);
      return;
    }
    for (const domain of domains) domain.apply(event);
  });
}

export async function reconcileAll(): Promise<void> {
  const snapshot = await getSession().snapshot();
  for (const domain of domains) domain.reconcile(snapshot);
}
