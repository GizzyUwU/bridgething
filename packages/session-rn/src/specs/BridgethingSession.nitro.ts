import type { HybridObject } from 'react-native-nitro-modules';

export type BridgethingSessionPeer = {
  id: string;
  name: string;
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

// flat with `kind` + per-kind optional fields; wire enum is adjacent-tagged and can't represent this shape directly.
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
  // native reuses the same filename on re-fetch; consumers diffing on uri must bust their cache when the icon changes.
  fileUri: string;
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

export interface BridgethingSession extends HybridObject<{ ios: 'swift'; android: 'kotlin' }> {
  start(): Promise<void>;
  stop(): Promise<void>;

  availableProviders(): Promise<BridgethingProviderInfo[]>;
  setActiveProvider(id: string | null): Promise<void>;
  currentProvider(): Promise<BridgethingProviderInfo | null>;
  cancelAuth(): Promise<void>;
  signOut(): Promise<void>;

  connectedPeers(): Promise<BridgethingSessionPeer[]>;
  currentNowPlaying(): Promise<BridgethingNowPlaying | null>;

  enableAncsNotifications(): Promise<BridgethingAncsSetupResult>;
  ancsAuthStatus(): Promise<BridgethingAncsAuthStatus>;

  listWebapps(deviceId: string): Promise<BridgethingWebappInfo[]>;
  currentWebapp(deviceId: string): Promise<BridgethingActiveWebapp | null>;
  // Swift ArrayBuffer typealias breaks Swift-to-C++ header interop; archive is passed as base64.
  installWebappFromBase64(deviceId: string, archiveBase64: string): Promise<BridgethingWebappInfo>;
  uninstallWebapp(deviceId: string, id: string): Promise<void>;
  switchWebapp(deviceId: string, id: string): Promise<void>;
  webappIcon(deviceId: string, id: string): Promise<BridgethingWebappIcon | null>;
  listWebappConfig(deviceId: string, id: string): Promise<BridgethingConfigEntry[]>;
  setWebappConfigField(deviceId: string, id: string, key: string, value: string): Promise<void>;
  deleteWebappConfigField(deviceId: string, id: string, key: string): Promise<void>;

  setCapabilityFlags(flags: BridgethingCapabilityFlags): Promise<void>;

  setOtaPollConfig(config: BridgethingOtaPollConfig | null): Promise<void>;
  pollOtaNow(): Promise<void>;
  deviceMeta(deviceId: string): Promise<BridgethingDeviceMeta | null>;

  hostInfo(): Promise<BridgethingHostInfo>;

  presentPairPicker(): Promise<BridgethingBtDevice | null>;

  // iOS uses ANCS instead and always returns false.
  isNotificationAccessGranted(): Promise<boolean>;
  // iOS rejects with `unsupported`.
  requestNotificationAccess(): Promise<void>;

  // iOS handles telephony over iAP2 and always returns false.
  isDefaultDialer(): Promise<boolean>;
  // iOS rejects with `unsupported`.
  requestDefaultDialer(): Promise<void>;

  revokeRuntimePermissions(permissions: string[]): Promise<boolean>;
  killApp(): Promise<void>;

  setOnProviderChanged(callback: (info: BridgethingProviderInfo | null) => void): void;
  setOnAuthStateChanged(callback: (state: BridgethingAuthState) => void): void;
  setOnPeerConnected(callback: (peer: BridgethingSessionPeer) => void): void;
  setOnPeerDisconnected(callback: (peerId: string) => void): void;
  setOnNowPlayingChanged(callback: (now: BridgethingNowPlaying | null) => void): void;
  setOnAncsAuthStatusChanged(callback: (status: BridgethingAncsAuthStatus) => void): void;
  setOnLog(callback: (level: string, message: string) => void): void;
  // every log line crosses JSI; real cost on a 512MB device -- enable only while a log UI is active.
  setLogStreamingEnabled(enabled: boolean): void;

  setOnWebappsChanged(callback: (deviceId: string) => void): void;
  setOnDeviceMetaChanged(callback: (deviceId: string, meta: BridgethingDeviceMeta) => void): void;
  setOnOtaEvent(callback: (event: BridgethingOtaEvent) => void): void;
}
