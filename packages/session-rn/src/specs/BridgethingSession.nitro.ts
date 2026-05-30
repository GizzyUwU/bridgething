import type { HybridObject } from 'react-native-nitro-modules';

export type BridgethingPeerLinkStatus = 'connected' | 'linkFailed';

export type BridgethingSessionPeer = {
  id: string;
  name: string;
  status: BridgethingPeerLinkStatus;
  linkError?: string;
};

export type BridgethingProviderInfo = {
  id: string;
  displayName: string;
  available: boolean;
};

export type BridgethingAuthKind = 'idle' | 'pending' | 'authenticated' | 'failed';

// flat instead of a discriminated union because Nitro's C++ codegen can't represent anonymous tagged unions.
export type BridgethingAuthState = {
  kind: BridgethingAuthKind;
  userCode?: string;
  verificationUrl?: string;
  verificationUrlComplete?: string;
  message?: string;
};

// build-injected Spotify auth config handed to the RN sign-in layer
export type BridgethingSpotifyAuthConfig = {
  scopes: string[];
  pkceClientId: string;
  pkceRedirectUri: string;
  pkceAuthorizeUrl: string;
  pkceTokenUrl: string;
  deviceCodePsk: string;
  deviceCodeUrl: string;
  deviceCodeTokenUrl: string;
  deviceCodeDescription: string;
};

// signed-in but degraded: the provider's API is throttling or unreachable.
// distinct from auth (tokens are still valid), so the UI keeps "signed in".
export type BridgethingServiceHealthKind = 'ok' | 'rateLimited' | 'unreachable';

export type BridgethingServiceHealth = {
  kind: BridgethingServiceHealthKind;
  retryAfterSeconds?: number;
};

export type BridgethingRepeatMode = 'off' | 'one' | 'all';

export type BridgethingNowPlayingTrack = {
  id?: string;
  title?: string;
  artist?: string;
  album?: string;
  artworkUrl?: string;
  durationMs?: number;
};

export type BridgethingNowPlayingPlayback = {
  playing: boolean;
  positionMs: number;
  shuffle: boolean;
  repeatMode: BridgethingRepeatMode;
};

export type BridgethingNowPlaying = {
  track?: BridgethingNowPlayingTrack;
  playback: BridgethingNowPlayingPlayback;
  appName?: string;
};

export type BridgethingAncsAuthStatus = 'unknown' | 'probing' | 'authorized' | 'unauthorized';

export type BridgethingAncsSetupKind = 'paired' | 'alreadyPaired' | 'cancelled' | 'unsupported' | 'failed';
export type BridgethingAncsSetupResult = {
  kind: BridgethingAncsSetupKind;
  authStatus: BridgethingAncsAuthStatus;
  message?: string;
};

export type BridgethingWebappSource = 'builtin' | 'installed';

export type BridgethingWebappRole = 'standard' | 'launcher';

export type BridgethingWebappInfo = {
  id: string;
  name: string;
  source: BridgethingWebappSource;
  role: BridgethingWebappRole;
  version: string;
  description?: string;
  iconAvailable: boolean;
  iconMime?: string;
  config: BridgethingConfigField[];
  permissions: string[];
};

export type BridgethingActiveWebapp = {
  id: string;
  name?: string;
};

export type BridgethingConfigKind = 'string' | 'number' | 'boolean' | 'enum' | 'secret';

export type BridgethingConfigField = {
  kind: BridgethingConfigKind;
  key: string;
  label: string;
  pattern?: string;
  minLength?: number;
  maxLength?: number;
  min?: number;
  max?: number;
  step?: number;
  choices?: string[];
  defaultValue?: string;
};

export type BridgethingConfigEntry = {
  key: string;
  value: string;
};

export type BridgethingWebappIcon = {
  fileUri?: string;
  svg?: string;
  mime?: string;
};

export type BridgethingCapabilityFlags = {
  geo: boolean;
  notifications: boolean;
  netFetch: boolean;
  netWs: boolean;
  audioTts: boolean;
};

export type BridgethingOtaPollConfig = {
  channel: string;
  intervalSeconds: number;
  autoPush: boolean;
  rootUrl?: string;
};

export type BridgethingOtaKind = 'image' | 'daemon' | 'builtinWebapp';

export type BridgethingOtaPhase =
  | 'idle'
  | 'streaming'
  | 'verifying'
  | 'writing'
  | 'confirming'
  | 'reboot'
  | 'completed'
  | 'failed';

export type BridgethingOtaEventKind =
  | 'manifestPolled'
  | 'manifestPollFailed'
  | 'channelMismatch'
  | 'updateAvailable'
  | 'progress'
  | 'updated'
  | 'failed';

export type BridgethingOtaEvent = {
  kind: BridgethingOtaEventKind;
  updatedAt?: string;
  reason?: string;
  deviceId?: string;
  otaKind?: BridgethingOtaKind;
  fromVersion?: string;
  toVersion?: string;
  phase?: BridgethingOtaPhase;
  percent?: number;
  deviceChannel?: string;
  configuredChannel?: string;
};

export type BridgethingOtaRelease = {
  version: string;
  daemonVersion: string;
  imageVersion: string;
  yanked: boolean;
  deprecated: boolean;
};

export type BridgethingOtaChannelInfo = {
  name: string;
  stability: string;
  isDefault: boolean;
  latest: string;
  releases: BridgethingOtaRelease[];
};

export type BridgethingOtaManifest = {
  updatedAt: string;
  channels: BridgethingOtaChannelInfo[];
};

export type BridgethingBtBondState = 'none' | 'bonding' | 'bonded';

// `address` is a BT MAC on Android and an opaque identifier on iOS.
export type BridgethingBtDevice = {
  address: string;
  name?: string;
  bondState: BridgethingBtBondState;
  isCarThing: boolean;
};

export type BridgethingDeviceMeta = {
  daemonVersion: string;
  imageVersion: string;
  appName: string;
  osName: string;
  osVersion: string;
  channel: string;
  modelName: string;
  serialNumber: string;
};

export type BridgethingHostInfo = {
  appName: string;
  appVersion: string;
  osName: string;
  osVersion: string;
  hostIdentifier: string;
  libVersion: string;
  libbridgethingVersion: string;
  adapterVersion: string;
};

export type BridgethingDeviceMetaEntry = {
  deviceId: string;
  meta: BridgethingDeviceMeta;
};

export type BridgethingSessionSnapshot = {
  hostInfo: BridgethingHostInfo;
  provider?: BridgethingProviderInfo;
  authState: BridgethingAuthState;
  serviceHealth: BridgethingServiceHealth;
  peers: BridgethingSessionPeer[];
  ancsAuthStatus: BridgethingAncsAuthStatus;
  nowPlaying?: BridgethingNowPlaying;
  deviceMeta: BridgethingDeviceMetaEntry[];
  capabilityFlags: BridgethingCapabilityFlags;
  otaPollConfig?: BridgethingOtaPollConfig;
};

export type BridgethingDiagKind = 'frame' | 'log' | 'breadcrumb';
export type BridgethingDiagDirection = 'outbound' | 'inbound';
export type BridgethingDiagFrameKind = 'request' | 'response' | 'event' | 'command';

export type BridgethingDiagEntry = {
  seq: number;
  ts: number;
  kind: BridgethingDiagKind;

  deviceId?: string;
  direction?: BridgethingDiagDirection;
  frameKind?: BridgethingDiagFrameKind;
  surface?: string;
  byteSize?: number;
  requestId?: string;
  latencyMs?: number;

  level?: string;
  target?: string;
  message?: string;

  category?: string;
  detail?: string;
  fields?: BridgethingConfigEntry[];
};

export type BridgethingCompanionDebug = {
  authorityPlaybackHeld: boolean;
  authorityMetadataHeld: boolean;
  baselinePollActive: boolean;
  hintFetchActive: boolean;
  ancsAuthStatus: BridgethingAncsAuthStatus;
};

export interface BridgethingSession extends HybridObject<{ ios: 'swift'; android: 'kotlin' }> {
  start(): Promise<void>;
  stop(): Promise<void>;

  availableProviders(): Promise<BridgethingProviderInfo[]>;
  setActiveProvider(id: string | null): Promise<void>;
  currentProvider(): Promise<BridgethingProviderInfo | null>;
  cancelAuth(): Promise<void>;
  signOut(): Promise<void>;

  spotifyAuthConfig(): Promise<BridgethingSpotifyAuthConfig>;
  completeSpotifySignIn(accessToken: string, refreshToken: string, usesDealer: boolean): Promise<void>;

  snapshot(): Promise<BridgethingSessionSnapshot>;

  diagnosticsSnapshot(limit: number): Promise<BridgethingDiagEntry[]>;
  companionDebug(): Promise<BridgethingCompanionDebug>;

  enableAncsNotifications(): Promise<BridgethingAncsSetupResult>;
  ancsAuthStatus(): Promise<BridgethingAncsAuthStatus>;

  listWebapps(deviceId: string): Promise<BridgethingWebappInfo[]>;
  currentWebapp(deviceId: string): Promise<BridgethingActiveWebapp | null>;
  installWebapp(deviceId: string, sourceUri: string): Promise<BridgethingWebappInfo>;
  uninstallWebapp(deviceId: string, id: string): Promise<void>;
  switchWebapp(deviceId: string, id: string): Promise<void>;
  webappIcon(deviceId: string, id: string): Promise<BridgethingWebappIcon | null>;
  listWebappConfig(deviceId: string, id: string): Promise<BridgethingConfigEntry[]>;
  setWebappConfigField(deviceId: string, id: string, key: string, value: string): Promise<void>;
  deleteWebappConfigField(deviceId: string, id: string, key: string): Promise<void>;

  setCapabilityFlags(flags: BridgethingCapabilityFlags): Promise<void>;

  setOtaPollConfig(config: BridgethingOtaPollConfig | null): Promise<void>;
  checkForOtaUpdate(channel: string, rootUrl: string | null): Promise<void>;
  fetchOtaManifest(rootUrl: string | null): Promise<BridgethingOtaManifest>;
  applyOtaUpdate(deviceId: string, channel: string, version: string, rootUrl: string | null): Promise<void>;

  reconnectPeer(deviceId: string): Promise<void>;

  presentPairPicker(): Promise<BridgethingBtDevice | null>;

  isNotificationAccessGranted(): Promise<boolean>;
  requestNotificationAccess(): Promise<void>;

  isDefaultDialer(): Promise<boolean>;
  requestDefaultDialer(): Promise<void>;

  revokeRuntimePermissions(permissions: string[]): Promise<boolean>;
  killApp(): Promise<void>;

  setOnProviderChanged(callback: (info: BridgethingProviderInfo | null) => void): void;
  setOnAuthStateChanged(callback: (state: BridgethingAuthState) => void): void;
  setOnServiceHealthChanged(callback: (health: BridgethingServiceHealth) => void): void;
  setOnPeerConnected(callback: (peer: BridgethingSessionPeer) => void): void;
  setOnPeerDisconnected(callback: (peerId: string) => void): void;
  setOnPeerLinkFailed(callback: (peer: BridgethingSessionPeer) => void): void;
  setOnNowPlayingChanged(callback: (now: BridgethingNowPlaying | null) => void): void;
  setOnAncsAuthStatusChanged(callback: (status: BridgethingAncsAuthStatus) => void): void;
  setOnLog(callback: (level: string, message: string) => void): void;
  setLogStreamingEnabled(enabled: boolean): void;
  setOnDiagEntry(callback: (entry: BridgethingDiagEntry) => void): void;

  setOnWebappsChanged(callback: (deviceId: string) => void): void;
  setOnDeviceMetaChanged(callback: (deviceId: string, meta: BridgethingDeviceMeta) => void): void;
  setOnOtaEvent(callback: (event: BridgethingOtaEvent) => void): void;
}
