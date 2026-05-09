import type {
  BridgethingActiveWebapp,
  BridgethingAncsAuthStatus,
  BridgethingAncsSetupResult,
  BridgethingAuthState,
  BridgethingCapabilityFlags,
  BridgethingConfigEntry,
  BridgethingDeviceMeta,
  BridgethingNowPlaying,
  BridgethingOtaEvent,
  BridgethingOtaPollConfig,
  BridgethingProviderInfo,
  BridgethingSessionPeer,
  BridgethingWebappIcon,
  BridgethingWebappInfo,
  BridgethingSession as NativeBridgethingSession,
} from './specs/BridgethingSession.nitro';

export type {
  BridgethingActiveWebapp,
  BridgethingAncsAuthStatus,
  BridgethingAncsSetupKind,
  BridgethingAncsSetupResult,
  BridgethingAuthKind,
  BridgethingAuthState,
  BridgethingCapabilityFlags,
  BridgethingConfigEntry,
  BridgethingConfigField,
  BridgethingDeviceMeta,
  BridgethingNowPlaying,
  BridgethingNowPlayingPlayback,
  BridgethingNowPlayingTrack,
  BridgethingOtaEvent,
  BridgethingOtaPollConfig,
  BridgethingProviderInfo,
  BridgethingRepeatMode,
  BridgethingSessionPeer,
  BridgethingWebappIcon,
  BridgethingWebappInfo,
} from './specs/BridgethingSession.nitro';

export type SessionListener<E> = (event: E) => void;

export type SessionEvent =
  | { type: 'providerChanged'; provider: BridgethingProviderInfo | null }
  | { type: 'authStateChanged'; state: BridgethingAuthState }
  | { type: 'peerConnected'; peer: BridgethingSessionPeer }
  | { type: 'peerDisconnected'; peerId: string }
  | { type: 'nowPlayingChanged'; nowPlaying: BridgethingNowPlaying | null }
  | { type: 'ancsAuthStatusChanged'; status: BridgethingAncsAuthStatus }
  | { type: 'webappsChanged'; deviceId: string }
  | { type: 'deviceMetaChanged'; deviceId: string; meta: BridgethingDeviceMeta }
  | { type: 'otaEvent'; event: BridgethingOtaEvent }
  | { type: 'log'; level: string; message: string };

export type BridgethingSessionOptions = {
  /** Override the underlying Nitro HybridObject. Tests use a fake. */
  native?: NativeBridgethingSession;
};

/** Pretty name for a peer — nickname when set, BT-advertised name otherwise. */
export function peerDisplayName(peer: BridgethingSessionPeer): string {
  return peer.nickname ?? peer.name;
}

/**
 * TS-side facade over the Nitro `BridgethingSession`. Centralizes the
 * native callback setters into a single `on(listener) -> off` pattern and
 * caches the latest provider / auth / peer / NowPlaying snapshots so
 * consumers don't have to bookkeep them manually.
 *
 * Multi-device: webapp / OTA methods take a `deviceId` matching one of
 * the ids returned by `connectedPeers()`. `session.device(id)` returns
 * a per-device convenience proxy with the deviceId pre-bound.
 */
export class BridgethingSession {
  private readonly native: NativeBridgethingSession;
  private readonly listeners: Set<SessionListener<SessionEvent>> = new Set();

  private currentProviderCache: BridgethingProviderInfo | null = null;
  private authStateCache: BridgethingAuthState = { kind: 'idle' };
  private peers: Map<string, BridgethingSessionPeer> = new Map();
  private nowPlayingCache: BridgethingNowPlaying | null = null;
  private ancsAuthStatusCache: BridgethingAncsAuthStatus = 'unknown';
  private deviceMetaCache: Map<string, BridgethingDeviceMeta> = new Map();

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

  // MARK: - Device naming

  async setDeviceNickname(deviceId: string, nickname: string | null): Promise<void> {
    await this.native.setDeviceNickname(deviceId, nickname);
  }

  async getDeviceNickname(deviceId: string): Promise<string | null> {
    return this.native.getDeviceNickname(deviceId);
  }

  // MARK: - Webapps (per-device)

  async listWebapps(deviceId: string): Promise<BridgethingWebappInfo[]> {
    return this.native.listWebapps(deviceId);
  }

  async currentWebapp(deviceId: string): Promise<BridgethingActiveWebapp | null> {
    return this.native.currentWebapp(deviceId);
  }

  async installWebappFromUrl(deviceId: string, url: string): Promise<BridgethingWebappInfo> {
    return this.native.installWebappFromUrl(deviceId, url);
  }

  async uninstallWebapp(deviceId: string, id: string): Promise<void> {
    await this.native.uninstallWebapp(deviceId, id);
  }

  async switchWebapp(deviceId: string, id: string): Promise<void> {
    await this.native.switchWebapp(deviceId, id);
  }

  async webappIcon(deviceId: string, id: string): Promise<BridgethingWebappIcon | null> {
    return this.native.webappIcon(deviceId, id);
  }

  async listWebappConfig(deviceId: string, id: string): Promise<BridgethingConfigEntry[]> {
    return this.native.listWebappConfig(deviceId, id);
  }

  async setWebappConfigField(deviceId: string, id: string, key: string, value: string): Promise<void> {
    await this.native.setWebappConfigField(deviceId, id, key, value);
  }

  async deleteWebappConfigField(deviceId: string, id: string, key: string): Promise<void> {
    await this.native.deleteWebappConfigField(deviceId, id, key);
  }

  // MARK: - Capability flags

  async getCapabilityFlags(): Promise<BridgethingCapabilityFlags> {
    return this.native.getCapabilityFlags();
  }

  async setCapabilityFlags(flags: BridgethingCapabilityFlags): Promise<void> {
    await this.native.setCapabilityFlags(flags);
  }

  // MARK: - OTA

  async setOtaPollConfig(config: BridgethingOtaPollConfig | null): Promise<void> {
    await this.native.setOtaPollConfig(config);
  }

  async getOtaPollConfig(): Promise<BridgethingOtaPollConfig | null> {
    return this.native.getOtaPollConfig();
  }

  async pollOtaNow(): Promise<void> {
    await this.native.pollOtaNow();
  }

  async deviceMeta(deviceId: string): Promise<BridgethingDeviceMeta | null> {
    return this.native.deviceMeta(deviceId);
  }

  // MARK: - Per-device proxy

  /** Convenience wrapper that pre-binds `deviceId` to every method. */
  device(deviceId: string): BridgethingDevice {
    return new BridgethingDevice(this, deviceId);
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

  /** Latest cached BridgeThingMeta for a device, or null if none seen. */
  cachedDeviceMeta(deviceId: string): BridgethingDeviceMeta | null {
    return this.deviceMetaCache.get(deviceId) ?? null;
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
      this.deviceMetaCache.delete(peerId);
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
    this.native.setOnWebappsChanged(deviceId => {
      this.dispatch({ type: 'webappsChanged', deviceId });
    });
    this.native.setOnDeviceMetaChanged((deviceId, meta) => {
      this.deviceMetaCache.set(deviceId, meta);
      this.dispatch({ type: 'deviceMetaChanged', deviceId, meta });
    });
    this.native.setOnOtaEvent(event => {
      this.dispatch({ type: 'otaEvent', event });
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

/**
 * Per-device convenience proxy returned by `session.device(id)`. Mirrors
 * the Swift gateway's `gateway.device(id).webapp.X()` shape so screens
 * working against a single device don't have to thread `deviceId`
 * through every call.
 */
export class BridgethingDevice {
  constructor(
    private readonly session: BridgethingSession,
    public readonly id: string,
  ) {}

  setNickname(nickname: string | null) {
    return this.session.setDeviceNickname(this.id, nickname);
  }
  getNickname() {
    return this.session.getDeviceNickname(this.id);
  }
  listWebapps() {
    return this.session.listWebapps(this.id);
  }
  currentWebapp() {
    return this.session.currentWebapp(this.id);
  }
  installFromUrl(url: string) {
    return this.session.installWebappFromUrl(this.id, url);
  }
  uninstall(webappId: string) {
    return this.session.uninstallWebapp(this.id, webappId);
  }
  switchTo(webappId: string) {
    return this.session.switchWebapp(this.id, webappId);
  }
  icon(webappId: string) {
    return this.session.webappIcon(this.id, webappId);
  }
  listConfig(webappId: string) {
    return this.session.listWebappConfig(this.id, webappId);
  }
  setConfigField(webappId: string, key: string, value: string) {
    return this.session.setWebappConfigField(this.id, webappId, key, value);
  }
  deleteConfigField(webappId: string, key: string) {
    return this.session.deleteWebappConfigField(this.id, webappId, key);
  }
  meta() {
    return this.session.deviceMeta(this.id);
  }
  cachedMeta() {
    return this.session.cachedDeviceMeta(this.id);
  }
}

function createNativeSession(): NativeBridgethingSession {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const { NitroModules } = require('react-native-nitro-modules') as typeof import('react-native-nitro-modules');
  return NitroModules.createHybridObject<NativeBridgethingSession>('BridgethingSession');
}
