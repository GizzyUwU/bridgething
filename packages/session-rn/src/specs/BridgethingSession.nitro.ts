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
  iconHash?: string;
  settingsHash?: string;
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

export type BridgethingDocEntry = {
  key: string;
  value: string;
};

export type BridgethingCapabilityFlags = {
  geo: boolean;
  notifications: boolean;
  netFetch: boolean;
  netWs: boolean;
  audioTts: boolean;
};

export type BridgethingOtaPollConfig = {
  intervalSeconds: number;
  autoPush: boolean;
  rootUrl?: string;
};

export type BridgethingOtaKind = 'image' | 'daemon' | 'builtinWebapp';

export type BridgethingOtaPhase =
  | 'idle'
  | 'downloading'
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
  | 'updateAvailable'
  | 'planned'
  | 'progress'
  | 'updated'
  | 'failed';

// one leg of the whole update: a companion-side R2 download, a BT stream to the
// device, the on-device write, or the reboot. `bytes` is the payload size for
// download/stream legs and the write payload proxy for apply; 0 when the leg has
// no meaningful byte count (reboot). the RN store turns these into time weights.
export type BridgethingOtaStepKind = 'download' | 'stream' | 'apply' | 'reboot';

export type BridgethingOtaStep = {
  id: number;
  kind: BridgethingOtaStepKind;
  label: string;
  bytes: number;
};

export type BridgethingOtaEvent = {
  kind: BridgethingOtaEventKind;
  updatedAt?: string;
  reason?: string;
  deviceId?: string;
  otaKind?: BridgethingOtaKind;
  fromVersion?: string;
  toVersion?: string;
  // the composite release an updateAvailable/planned run targets, broken out.
  releaseVersion?: string;
  daemonVersion?: string;
  imageVersion?: string;
  // full ordered leg list, present on `planned` so the store can weight the run.
  steps?: BridgethingOtaStep[];
  // which leg a `progress` event belongs to, indexing into the plan's steps.
  stepId?: number;
  phase?: BridgethingOtaPhase;
  percent?: number;
  // during the apply leg: how far swupdate is through pulling zck deltas over BT (0-100).
  // 100 (or absent) means the pull is done and the eMMC write is underway.
  dwlPercent?: number;
  stageAsset?: string;
  stageReceived?: number;
  stageTotal?: number;
  stageRatePerSec?: number;
  stageEtaSeconds?: number;
};

export type BridgethingOtaRelease = {
  version: string;
  daemonVersion: string;
  imageVersion: string;
  yanked: boolean;
  deprecated: boolean;
};

export type BridgethingOtaChannelInfo = {
  slug: string;
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

export type BridgethingCatalogPollConfig = {
  intervalSeconds: number;
  autoInstall: boolean;
};

export type BridgethingCatalogEventKind =
  | 'refreshed'
  | 'sourceFailed'
  | 'updateAvailable'
  | 'installed'
  | 'installFailed';

// flat for the same reason as BridgethingOtaEvent: Nitro can't represent a tagged union.
export type BridgethingCatalogEvent = {
  kind: BridgethingCatalogEventKind;
  sourceCount?: number;
  appCount?: number;
  url?: string;
  reason?: string;
  deviceId?: string;
  appId?: string;
  name?: string;
  fromVersion?: string;
  toVersion?: string;
  version?: string;
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
  nickname?: string;
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

export type BridgethingDeviceLogLine = {
  seq: number;
  ts: number;
  level: string;
  message: string;
};

export type BridgethingCompanionDebug = {
  authorityPlaybackHeld: boolean;
  authorityMetadataHeld: boolean;
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

  snapshot(): Promise<BridgethingSessionSnapshot>;

  deviceLogSnapshot(limit: number): Promise<BridgethingDeviceLogLine[]>;
  companionDebug(): Promise<BridgethingCompanionDebug>;

  enableAncsNotifications(): Promise<BridgethingAncsSetupResult>;
  ancsAuthStatus(): Promise<BridgethingAncsAuthStatus>;

  listWebapps(deviceId: string): Promise<BridgethingWebappInfo[]>;
  currentWebapp(deviceId: string): Promise<BridgethingActiveWebapp | null>;
  installWebapp(deviceId: string, sourceUri: string): Promise<BridgethingWebappInfo>;
  uninstallWebapp(deviceId: string, id: string): Promise<void>;
  switchWebapp(deviceId: string, id: string): Promise<void>;
  webappIcon(deviceId: string, id: string): Promise<BridgethingWebappIcon | null>;
  webappSettingsPage(deviceId: string, id: string): Promise<string>;
  listWebappConfig(deviceId: string, id: string): Promise<BridgethingConfigEntry[]>;
  setWebappConfigField(deviceId: string, id: string, key: string, value: string): Promise<void>;
  deleteWebappConfigField(deviceId: string, id: string, key: string): Promise<void>;
  getWebappDoc(deviceId: string, id: string, key: string): Promise<string | null>;
  listWebappDoc(deviceId: string, id: string): Promise<BridgethingDocEntry[]>;
  setWebappDoc(deviceId: string, id: string, key: string, value: string): Promise<void>;
  deleteWebappDoc(deviceId: string, id: string, key: string): Promise<void>;

  setCapabilityFlags(flags: BridgethingCapabilityFlags): Promise<void>;

  setDeviceAutoResume(deviceId: string, enabled: boolean): Promise<void>;
  isDeviceAutoResumeEnabled(deviceId: string): Promise<boolean>;

  setOtaPollConfig(config: BridgethingOtaPollConfig | null): Promise<void>;
  checkForOtaUpdate(rootUrl: string | null): Promise<void>;
  fetchOtaManifest(rootUrl: string | null): Promise<BridgethingOtaManifest>;
  applyOtaUpdate(deviceId: string, channel: string, version: string, rootUrl: string | null): Promise<void>;

  catalogSources(): Promise<string[]>;
  addCatalogSource(url: string): Promise<void>;
  removeCatalogSource(url: string): Promise<void>;
  refreshCatalog(): Promise<void>;
  availableCatalogApps(deviceId: string): Promise<string>;
  checkForCatalogUpdates(deviceId: string): Promise<string>;
  installCatalogApp(
    deviceId: string,
    appId: string,
    version: string,
    sourceUrl: string,
  ): Promise<BridgethingWebappInfo>;
  setCatalogPollConfig(config: BridgethingCatalogPollConfig | null): Promise<void>;

  reconnectPeer(deviceId: string): Promise<void>;

  // empty string clears; the daemon broadcasts the change back as a deviceMetaChanged update
  deviceSetNickname(deviceId: string, nickname: string): Promise<void>;

  presentPairPicker(): Promise<BridgethingBtDevice | null>;

  isNotificationAccessGranted(): Promise<boolean>;
  requestNotificationAccess(): Promise<void>;

  isDefaultDialer(): Promise<boolean>;
  requestDefaultDialer(): Promise<void>;

  forgetCompanionDevice(mac: string): Promise<void>;

  isIgnoringBatteryOptimizations(): Promise<boolean>;
  requestIgnoreBatteryOptimizations(): Promise<void>;

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
  setLocalLogStreamingEnabled(enabled: boolean): void;

  setOnWebappsChanged(callback: (deviceId: string) => void): void;
  setOnWebappDocChanged(
    callback: (deviceId: string, webappId: string, key: string, value: string | null) => void,
  ): void;
  setOnDeviceMetaChanged(callback: (deviceId: string, meta: BridgethingDeviceMeta) => void): void;
  setOnOtaEvent(callback: (event: BridgethingOtaEvent) => void): void;
  setOnCatalogEvent(callback: (event: BridgethingCatalogEvent) => void): void;
}
