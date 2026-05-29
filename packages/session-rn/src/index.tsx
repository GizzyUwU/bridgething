import type {
  BridgethingActiveWebapp,
  BridgethingAncsAuthStatus,
  BridgethingAncsSetupResult,
  BridgethingAuthState,
  BridgethingBtDevice,
  BridgethingCapabilityFlags,
  BridgethingConfigEntry,
  BridgethingDeviceMeta,
  BridgethingHostInfo,
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
  BridgethingBtBondState,
  BridgethingBtDevice,
  BridgethingCapabilityFlags,
  BridgethingConfigEntry,
  BridgethingConfigField,
  BridgethingDeviceMeta,
  BridgethingHostInfo,
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

export class BridgethingSession {
  private readonly native: NativeBridgethingSession;
  private readonly listeners: Set<(event: SessionEvent) => void> = new Set();
  private logSubscriberCount = 0;

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
    if (enabled) {
      this.logSubscriberCount += 1;
      if (this.logSubscriberCount === 1) {
        this.native.setLogStreamingEnabled(true);
      }
    } else {
      this.logSubscriberCount = Math.max(0, this.logSubscriberCount - 1);
      if (this.logSubscriberCount === 0) {
        this.native.setLogStreamingEnabled(false);
      }
    }
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

  async installWebappFromBytes(deviceId: string, archive: ArrayBuffer): Promise<BridgethingWebappInfo> {
    return this.native.installWebappFromBase64(deviceId, arrayBufferToBase64(archive));
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

  async setOtaPollConfig(config: BridgethingOtaPollConfig | null): Promise<void> {
    await this.native.setOtaPollConfig(config);
  }

  async pollOtaNow(): Promise<void> {
    await this.native.pollOtaNow();
  }

  async deviceMeta(deviceId: string): Promise<BridgethingDeviceMeta | null> {
    return this.native.deviceMeta(deviceId);
  }

  async hostInfo(): Promise<BridgethingHostInfo> {
    return this.native.hostInfo();
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
    this.native.setOnPeerConnected(peer => {
      this.dispatch({ type: 'peerConnected', peer });
    });
    this.native.setOnPeerDisconnected(peerId => {
      this.dispatch({ type: 'peerDisconnected', peerId });
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
  installFromBytes(archive: ArrayBuffer) {
    return this.session.installWebappFromBytes(this.id, archive);
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
}

// Swift ArrayBuffer typealias breaks Swift-to-C++ header interop; base64 string surface avoids it.
function arrayBufferToBase64(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  // 8 KiB chunks avoid the JS call-stack limit on String.fromCharCode
  let binary = '';
  const chunk = 8192;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode.apply(null, Array.from(bytes.subarray(i, i + chunk)));
  }
  // test environments may not ship btoa
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const g = globalThis as any;
  if (typeof g.btoa === 'function') return g.btoa(binary) as string;
  return manualBtoa(binary);
}

const B64_ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';

function manualBtoa(binary: string): string {
  let out = '';
  let i = 0;
  while (i < binary.length) {
    const a = binary.charCodeAt(i++);
    const b = i < binary.length ? binary.charCodeAt(i++) : NaN;
    const c = i < binary.length ? binary.charCodeAt(i++) : NaN;
    out += B64_ALPHABET[a >> 2];
    out += B64_ALPHABET[((a & 3) << 4) | (isNaN(b) ? 0 : b >> 4)];
    out += isNaN(b) ? '=' : B64_ALPHABET[((b & 15) << 2) | (isNaN(c) ? 0 : c >> 6)];
    out += isNaN(c) ? '=' : B64_ALPHABET[c & 63];
  }
  return out;
}

function createNativeSession(): NativeBridgethingSession {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const { NitroModules } = require('react-native-nitro-modules') as typeof import('react-native-nitro-modules');
  return NitroModules.createHybridObject<NativeBridgethingSession>('BridgethingSession');
}
