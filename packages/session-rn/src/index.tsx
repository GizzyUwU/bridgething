import type {
  BridgethingAuthState,
  BridgethingProviderInfo,
  BridgethingSessionPeer,
  BridgethingSession as NativeBridgethingSession,
} from './specs/BridgethingSession.nitro';

export type {
  BridgethingAuthKind,
  BridgethingAuthState,
  BridgethingProviderInfo,
  BridgethingSessionPeer,
} from './specs/BridgethingSession.nitro';

export type SessionListener<E> = (event: E) => void;

export type SessionEvent =
  | { type: 'providerChanged'; provider: BridgethingProviderInfo | null }
  | { type: 'authStateChanged'; state: BridgethingAuthState }
  | { type: 'peerConnected'; peer: BridgethingSessionPeer }
  | { type: 'peerDisconnected'; peerId: string }
  | { type: 'log'; level: string; message: string };

export type BridgethingSessionOptions = {
  /** Override the underlying Nitro HybridObject. Tests use a fake. */
  native?: NativeBridgethingSession;
};

/**
 * TS-side facade over the Nitro `BridgethingSession`. Centralizes the
 * native callback setters into a single `on(listener) -> off` pattern and
 * caches the latest provider / auth / peer snapshots so consumers don't
 * have to bookkeep them manually.
 */
export class BridgethingSession {
  private readonly native: NativeBridgethingSession;
  private readonly listeners: Set<SessionListener<SessionEvent>> = new Set();

  private currentProviderCache: BridgethingProviderInfo | null = null;
  private authStateCache: BridgethingAuthState = { kind: 'idle' };
  private peers: Map<string, BridgethingSessionPeer> = new Map();

  constructor(options: BridgethingSessionOptions = {}) {
    this.native = options.native ?? createNativeSession();
    this.wire();
  }

  on(listener: SessionListener<SessionEvent>): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  async start(): Promise<void> {
    await this.native.start();
  }

  async stop(): Promise<void> {
    await this.native.stop();
  }

  async availableProviders(): Promise<BridgethingProviderInfo[]> {
    return this.native.availableProviders();
  }

  async setActiveProvider(id: string | null): Promise<void> {
    await this.native.setActiveProvider(id);
  }

  async currentProvider(): Promise<BridgethingProviderInfo | null> {
    return this.native.currentProvider();
  }

  async connectedPeers(): Promise<BridgethingSessionPeer[]> {
    return this.native.connectedPeers();
  }

  /** Latest cached active provider; null if none or before first event. */
  get cachedProvider(): BridgethingProviderInfo | null {
    return this.currentProviderCache;
  }

  /** Latest cached auth state; `idle` before any event lands. */
  get cachedAuthState(): BridgethingAuthState {
    return this.authStateCache;
  }

  /** Snapshot of currently-cached connected peers. */
  get cachedPeers(): BridgethingSessionPeer[] {
    return Array.from(this.peers.values());
  }

  private wire(): void {
    this.native.setOnProviderChanged(provider => {
      this.currentProviderCache = provider;
      this.dispatch({ type: 'providerChanged', provider });
    });
    this.native.setOnAuthStateChanged(state => {
      this.authStateCache = state;
      this.dispatch({ type: 'authStateChanged', state });
    });
    this.native.setOnPeerConnected(peer => {
      this.peers.set(peer.id, peer);
      this.dispatch({ type: 'peerConnected', peer });
    });
    this.native.setOnPeerDisconnected(peerId => {
      this.peers.delete(peerId);
      this.dispatch({ type: 'peerDisconnected', peerId });
    });
    this.native.setOnLog((level, message) => {
      this.dispatch({ type: 'log', level, message });
    });
  }

  private dispatch(event: SessionEvent): void {
    for (const listener of this.listeners) {
      try {
        listener(event);
      } catch (err) {
        console.error('[bridgething] session listener threw', err);
      }
    }
  }
}

function createNativeSession(): NativeBridgethingSession {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const { NitroModules } = require('react-native-nitro-modules') as typeof import('react-native-nitro-modules');
  return NitroModules.createHybridObject<NativeBridgethingSession>('BridgethingSession');
}
