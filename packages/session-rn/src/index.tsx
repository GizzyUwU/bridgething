import type {
  BridgethingAncsAuthStatus,
  BridgethingAncsSetupResult,
  BridgethingAuthState,
  BridgethingNowPlaying,
  BridgethingProviderInfo,
  BridgethingSessionPeer,
  BridgethingSession as NativeBridgethingSession,
} from './specs/BridgethingSession.nitro';

export type {
  BridgethingAncsAuthStatus,
  BridgethingAncsSetupKind,
  BridgethingAncsSetupResult,
  BridgethingAuthKind,
  BridgethingAuthState,
  BridgethingNowPlaying,
  BridgethingNowPlayingPlayback,
  BridgethingNowPlayingTrack,
  BridgethingProviderInfo,
  BridgethingRepeatMode,
  BridgethingSessionPeer,
} from './specs/BridgethingSession.nitro';

export type SessionListener<E> = (event: E) => void;

export type SessionEvent =
  | { type: 'providerChanged'; provider: BridgethingProviderInfo | null }
  | { type: 'authStateChanged'; state: BridgethingAuthState }
  | { type: 'peerConnected'; peer: BridgethingSessionPeer }
  | { type: 'peerDisconnected'; peerId: string }
  | { type: 'nowPlayingChanged'; nowPlaying: BridgethingNowPlaying | null }
  | { type: 'ancsAuthStatusChanged'; status: BridgethingAncsAuthStatus }
  | { type: 'log'; level: string; message: string };

export type BridgethingSessionOptions = {
  /** Override the underlying Nitro HybridObject. Tests use a fake. */
  native?: NativeBridgethingSession;
};

/**
 * TS-side facade over the Nitro `BridgethingSession`. Centralizes the
 * native callback setters into a single `on(listener) -> off` pattern and
 * caches the latest provider / auth / peer / NowPlaying snapshots so
 * consumers don't have to bookkeep them manually.
 */
export class BridgethingSession {
  private readonly native: NativeBridgethingSession;
  private readonly listeners: Set<SessionListener<SessionEvent>> = new Set();

  private currentProviderCache: BridgethingProviderInfo | null = null;
  private authStateCache: BridgethingAuthState = { kind: 'idle' };
  private peers: Map<string, BridgethingSessionPeer> = new Map();
  private nowPlayingCache: BridgethingNowPlaying | null = null;
  private ancsAuthStatusCache: BridgethingAncsAuthStatus = 'unknown';

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

  async cancelAuth(): Promise<void> {
    await this.native.cancelAuth();
  }

  async signOut(): Promise<void> {
    await this.native.signOut();
  }

  async currentProvider(): Promise<BridgethingProviderInfo | null> {
    return this.native.currentProvider();
  }

  async connectedPeers(): Promise<BridgethingSessionPeer[]> {
    return this.native.connectedPeers();
  }

  async currentNowPlaying(): Promise<BridgethingNowPlaying | null> {
    return this.native.currentNowPlaying();
  }

  /**
   * Drive the iOS AccessorySetupKit pair flow that creates the LE bond
   * required for ANCS. Resolves once the picker-side outcome is known;
   * the daemon-observed `authStatus` may transition asynchronously after
   * — listen via `on(...)` for `ancsAuthStatusChanged` events.
   */
  async enableAncsNotifications(): Promise<BridgethingAncsSetupResult> {
    return this.native.enableAncsNotifications();
  }

  async ancsAuthStatus(): Promise<BridgethingAncsAuthStatus> {
    return this.native.ancsAuthStatus();
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

  /** Latest cached NowPlaying mirror; null if nothing playing. */
  get cachedNowPlaying(): BridgethingNowPlaying | null {
    return this.nowPlayingCache;
  }

  /** Latest cached daemon-reported ANCS auth status. */
  get cachedAncsAuthStatus(): BridgethingAncsAuthStatus {
    return this.ancsAuthStatusCache;
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
    this.native.setOnNowPlayingChanged(nowPlaying => {
      this.nowPlayingCache = nowPlaying;
      this.dispatch({ type: 'nowPlayingChanged', nowPlaying });
    });
    this.native.setOnAncsAuthStatusChanged(status => {
      this.ancsAuthStatusCache = status;
      this.dispatch({ type: 'ancsAuthStatusChanged', status });
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
