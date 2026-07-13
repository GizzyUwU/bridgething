import type {
  BridgethingActiveWebapp,
  BridgethingAncsAuthStatus,
  BridgethingAncsSetupResult,
  BridgethingAuthState,
  BridgethingBtDevice,
  BridgethingCapabilityFlags,
  BridgethingCatalogEvent,
  BridgethingCatalogPollConfig,
  BridgethingCompanionDebug,
  BridgethingConfigEntry,
  BridgethingDeviceLogLine,
  BridgethingDeviceMeta,
  BridgethingNowPlaying,
  BridgethingOtaEvent,
  BridgethingOtaManifest,
  BridgethingOtaPollConfig,
  BridgethingProviderInfo,
  BridgethingServiceHealth,
  BridgethingSessionPeer,
  BridgethingSessionSnapshot,
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
  BridgethingBtBondState,
  BridgethingBtDevice,
  BridgethingCapabilityFlags,
  BridgethingCatalogEvent,
  BridgethingCatalogEventKind,
  BridgethingCatalogPollConfig,
  BridgethingCompanionDebug,
  BridgethingConfigEntry,
  BridgethingConfigField,
  BridgethingDeviceLogLine,
  BridgethingDeviceMeta,
  BridgethingDeviceMetaEntry,
  BridgethingHostInfo,
  BridgethingNowPlaying,
  BridgethingNowPlayingPlayback,
  BridgethingNowPlayingTrack,
  BridgethingOtaChannelInfo,
  BridgethingOtaEvent,
  BridgethingOtaEventKind,
  BridgethingOtaKind,
  BridgethingOtaManifest,
  BridgethingOtaPhase,
  BridgethingOtaPollConfig,
  BridgethingOtaRelease,
  BridgethingOtaStep,
  BridgethingOtaStepKind,
  BridgethingPeerLinkStatus,
  BridgethingProviderInfo,
  BridgethingRepeatMode,
  BridgethingServiceHealth,
  BridgethingServiceHealthKind,
  BridgethingSessionPeer,
  BridgethingSessionSnapshot,
  BridgethingWebappIcon,
  BridgethingWebappInfo,
} from './specs/BridgethingSession.nitro';

export type CatalogDownload = { url: string; size: number; sha256: string };

export type CatalogVersion = {
  version: string;
  released_at: string;
  download: CatalogDownload;
  permissions: string[];
  min_libbridgething_version: string;
  changelog?: string | null;
};

export type CatalogApp = {
  id: string;
  name: string;
  description: string;
  author: string;
  icon?: string | null;
  homepage?: string | null;
  source?: string | null;
  versions: CatalogVersion[];
};

export type CatalogListing = {
  app: CatalogApp;
  sourceUrl: string;
  newestCompatible?: CatalogVersion | null;
  installedVersion?: string | null;
  updateAvailable: boolean;
  alsoAvailableFrom: string[];
};

export type CatalogUpdate = {
  appId: string;
  name: string;
  installedVersion: string;
  target: CatalogVersion;
  sourceUrl: string;
};

export type SessionEvent =
  | { type: 'providerChanged'; provider: BridgethingProviderInfo | null }
  | { type: 'authStateChanged'; state: BridgethingAuthState }
  | { type: 'serviceHealthChanged'; health: BridgethingServiceHealth }
  | { type: 'peerConnected'; peer: BridgethingSessionPeer }
  | { type: 'peerDisconnected'; peerId: string }
  | { type: 'peerLinkFailed'; peer: BridgethingSessionPeer }
  | { type: 'nowPlayingChanged'; nowPlaying: BridgethingNowPlaying | null }
  | { type: 'ancsAuthStatusChanged'; status: BridgethingAncsAuthStatus }
  | { type: 'webappsChanged'; deviceId: string }
  | { type: 'deviceMetaChanged'; deviceId: string; meta: BridgethingDeviceMeta }
  | { type: 'otaEvent'; event: BridgethingOtaEvent }
  | { type: 'catalogEvent'; event: BridgethingCatalogEvent }
  | { type: 'log'; level: string; message: string };

export class BridgethingSession {
  private readonly native: NativeBridgethingSession;
  private readonly listeners: Set<(event: SessionEvent) => void> = new Set();
  private logStreamingEnabled = false;
  private localLogStreamingEnabled = false;

  constructor(options: { native?: NativeBridgethingSession } = {}) {
    this.native = options.native ?? createNativeSession();
    this.wire();
  }

  subscribe(listener: (event: SessionEvent) => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  setLogStreamingEnabled(enabled: boolean): void {
    if (enabled === this.logStreamingEnabled) return;
    this.logStreamingEnabled = enabled;
    this.native.setLogStreamingEnabled(enabled);
  }

  setLocalLogStreamingEnabled(enabled: boolean): void {
    if (enabled === this.localLogStreamingEnabled) return;
    this.localLogStreamingEnabled = enabled;
    this.native.setLocalLogStreamingEnabled(enabled);
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

  async snapshot(): Promise<BridgethingSessionSnapshot> {
    return this.native.snapshot();
  }

  async deviceLogSnapshot(limit: number): Promise<BridgethingDeviceLogLine[]> {
    return this.native.deviceLogSnapshot(limit);
  }

  async companionDebug(): Promise<BridgethingCompanionDebug> {
    return this.native.companionDebug();
  }

  async enableAncsNotifications(): Promise<BridgethingAncsSetupResult> {
    return this.native.enableAncsNotifications();
  }

  async ancsAuthStatus(): Promise<BridgethingAncsAuthStatus> {
    return this.native.ancsAuthStatus();
  }

  async listWebapps(deviceId: string): Promise<BridgethingWebappInfo[]> {
    return this.native.listWebapps(deviceId);
  }

  async currentWebapp(deviceId: string): Promise<BridgethingActiveWebapp | null> {
    return this.native.currentWebapp(deviceId);
  }

  async installWebappFromUri(deviceId: string, sourceUri: string): Promise<BridgethingWebappInfo> {
    return this.native.installWebapp(deviceId, sourceUri);
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

  async setCapabilityFlags(flags: BridgethingCapabilityFlags): Promise<void> {
    await this.native.setCapabilityFlags(flags);
  }

  async setDeviceAutoResume(deviceId: string, enabled: boolean): Promise<void> {
    await this.native.setDeviceAutoResume(deviceId, enabled);
  }

  async isDeviceAutoResumeEnabled(deviceId: string): Promise<boolean> {
    return this.native.isDeviceAutoResumeEnabled(deviceId);
  }

  async setOtaPollConfig(config: BridgethingOtaPollConfig | null): Promise<void> {
    await this.native.setOtaPollConfig(config);
  }

  async checkForOtaUpdate(rootUrl: string | null = null): Promise<void> {
    await this.native.checkForOtaUpdate(rootUrl);
  }

  async fetchOtaManifest(rootUrl: string | null = null): Promise<BridgethingOtaManifest> {
    return this.native.fetchOtaManifest(rootUrl);
  }

  async applyOtaUpdate(
    deviceId: string,
    channel: string,
    version: string,
    rootUrl: string | null = null,
  ): Promise<void> {
    await this.native.applyOtaUpdate(deviceId, channel, version, rootUrl);
  }

  async catalogSources(): Promise<string[]> {
    return this.native.catalogSources();
  }

  async addCatalogSource(url: string): Promise<void> {
    await this.native.addCatalogSource(url);
  }

  async removeCatalogSource(url: string): Promise<void> {
    await this.native.removeCatalogSource(url);
  }

  async refreshCatalog(): Promise<void> {
    await this.native.refreshCatalog();
  }

  async availableApps(deviceId: string): Promise<CatalogListing[]> {
    return JSON.parse(await this.native.availableCatalogApps(deviceId)) as CatalogListing[];
  }

  async checkForCatalogUpdates(deviceId: string): Promise<CatalogUpdate[]> {
    return JSON.parse(await this.native.checkForCatalogUpdates(deviceId)) as CatalogUpdate[];
  }

  async installCatalogApp(
    deviceId: string,
    appId: string,
    version: string,
    sourceUrl: string,
  ): Promise<BridgethingWebappInfo> {
    return this.native.installCatalogApp(deviceId, appId, version, sourceUrl);
  }

  async setCatalogPollConfig(config: BridgethingCatalogPollConfig | null): Promise<void> {
    await this.native.setCatalogPollConfig(config);
  }

  async reconnectPeer(deviceId: string): Promise<void> {
    await this.native.reconnectPeer(deviceId);
  }

  async deviceSetNickname(deviceId: string, nickname: string): Promise<void> {
    await this.native.deviceSetNickname(deviceId, nickname);
  }

  async presentPairPicker(): Promise<BridgethingBtDevice | null> {
    return this.native.presentPairPicker();
  }

  async isNotificationAccessGranted(): Promise<boolean> {
    return this.native.isNotificationAccessGranted();
  }

  async requestNotificationAccess(): Promise<void> {
    await this.native.requestNotificationAccess();
  }

  async isDefaultDialer(): Promise<boolean> {
    return this.native.isDefaultDialer();
  }

  async requestDefaultDialer(): Promise<void> {
    await this.native.requestDefaultDialer();
  }

  async forgetCompanionDevice(mac: string): Promise<void> {
    await this.native.forgetCompanionDevice(mac);
  }

  async isIgnoringBatteryOptimizations(): Promise<boolean> {
    return this.native.isIgnoringBatteryOptimizations();
  }

  async requestIgnoreBatteryOptimizations(): Promise<void> {
    await this.native.requestIgnoreBatteryOptimizations();
  }

  async revokeRuntimePermissions(permissions: string[]): Promise<boolean> {
    return this.native.revokeRuntimePermissions(permissions);
  }

  async killApp(): Promise<void> {
    await this.native.killApp();
  }

  device(deviceId: string): BridgethingDevice {
    return new BridgethingDevice(this, deviceId);
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

  private wire(): void {
    this.native.setOnProviderChanged(provider => {
      this.dispatch({ type: 'providerChanged', provider });
    });
    this.native.setOnAuthStateChanged(state => {
      this.dispatch({ type: 'authStateChanged', state });
    });
    this.native.setOnServiceHealthChanged(health => {
      this.dispatch({ type: 'serviceHealthChanged', health });
    });
    this.native.setOnPeerConnected(peer => {
      this.dispatch({ type: 'peerConnected', peer });
    });
    this.native.setOnPeerDisconnected(peerId => {
      this.dispatch({ type: 'peerDisconnected', peerId });
    });
    this.native.setOnPeerLinkFailed(peer => {
      this.dispatch({ type: 'peerLinkFailed', peer });
    });
    this.native.setOnNowPlayingChanged(nowPlaying => {
      this.dispatch({ type: 'nowPlayingChanged', nowPlaying });
    });
    this.native.setOnAncsAuthStatusChanged(status => {
      this.dispatch({ type: 'ancsAuthStatusChanged', status });
    });
    this.native.setOnWebappsChanged(deviceId => {
      this.dispatch({ type: 'webappsChanged', deviceId });
    });
    this.native.setOnDeviceMetaChanged((deviceId, meta) => {
      this.dispatch({ type: 'deviceMetaChanged', deviceId, meta });
    });
    this.native.setOnOtaEvent(event => {
      this.dispatch({ type: 'otaEvent', event });
    });
    this.native.setOnCatalogEvent(event => {
      this.dispatch({ type: 'catalogEvent', event });
    });
    this.native.setOnLog((level, message) => {
      this.dispatch({ type: 'log', level, message });
    });
  }
}

export class BridgethingDevice {
  constructor(
    private readonly session: BridgethingSession,
    public readonly id: string,
  ) {}

  listWebapps() {
    return this.session.listWebapps(this.id);
  }
  currentWebapp() {
    return this.session.currentWebapp(this.id);
  }
  installFromUri(sourceUri: string) {
    return this.session.installWebappFromUri(this.id, sourceUri);
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
  availableApps() {
    return this.session.availableApps(this.id);
  }
  installApp(appId: string, version: string, sourceUrl: string) {
    return this.session.installCatalogApp(this.id, appId, version, sourceUrl);
  }
  checkForCatalogUpdates() {
    return this.session.checkForCatalogUpdates(this.id);
  }
}

function createNativeSession(): NativeBridgethingSession {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const { NitroModules } = require('react-native-nitro-modules') as typeof import('react-native-nitro-modules');
  return NitroModules.createHybridObject<NativeBridgethingSession>('BridgethingSession');
}
