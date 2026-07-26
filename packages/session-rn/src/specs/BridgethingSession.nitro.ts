import type { HybridObject } from 'react-native-nitro-modules';

export type BridgethingPeerLinkStatus = 'connected' | 'linkFailed';

export type BridgethingSessionPeer = {
  id: string;
  name: string;
  status: BridgethingPeerLinkStatus;
  linkError?: string;
};

export type BridgethingAuthKind = 'idle' | 'pending' | 'authenticated' | 'failed';

export type BridgethingAuthState = {
  kind: BridgethingAuthKind;
  userCode?: string;
  verificationUrl?: string;
  verificationUrlComplete?: string;
  message?: string;
};

export type BridgethingServiceHealthKind = 'ok' | 'rateLimited' | 'unreachable';

export type BridgethingServiceHealth = {
  kind: BridgethingServiceHealthKind;
  retryAfterSeconds?: number;
};

export type BridgethingProviderInfo = {
  id: string;
  displayName: string;
  available: boolean;
  connected: boolean;
  authState: BridgethingAuthState;
  serviceHealth: BridgethingServiceHealth;
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
  provenance?: string;
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
  releaseVersion?: string;
  daemonVersion?: string;
  imageVersion?: string;
  steps?: BridgethingOtaStep[];
  stepId?: number;
  phase?: BridgethingOtaPhase;
  percent?: number;
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

export type BridgethingBtBondState = 'none' | 'bonding' | 'bonded';

export type BridgethingBtDevice = {
  address: string;
  name?: string;
  bondState: BridgethingBtBondState;
  isCarThing: boolean;
};

export type BridgethingDeviceMeta = {
  daemonVersion: string;
  libbridgethingVersion: string;
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
  providers: BridgethingProviderInfo[];
  providerPriority: string[];
  libraryProvider?: string;
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

export type BridgethingLogArchive = {
  id: string;
  startedAt: number;
  bytes: number;
  pinned: boolean;
  current: boolean;
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
  connectProvider(id: string): Promise<void>;
  disconnectProvider(id: string): Promise<void>;
  cancelAuth(id: string): Promise<void>;
  setProviderPriority(ids: string[]): Promise<void>;

  snapshot(): Promise<BridgethingSessionSnapshot>;

  deviceLogSnapshot(limit: number): Promise<BridgethingDeviceLogLine[]>;
  companionDebug(): Promise<BridgethingCompanionDebug>;

  persistedLogSize(): Promise<number>;
  logArchives(): Promise<BridgethingLogArchive[]>;
  exportLogs(archiveId: string | null): Promise<string>;
  shareLogs(archiveId: string | null): Promise<boolean>;
  deleteLogArchive(archiveId: string): Promise<void>;
  clearPersistedLogs(): Promise<void>;

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

  installWebappFromUrl(
    deviceId: string,
    url: string,
    sha256: string,
    size: number,
    provenance: string | null,
  ): Promise<BridgethingWebappInfo>;

  reconnectPeer(deviceId: string): Promise<void>;

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

  setOnProvidersChanged(callback: (providers: BridgethingProviderInfo[]) => void): void;
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
}
