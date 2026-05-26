import type { HybridObject } from 'react-native-nitro-modules';

/** Connected Car Thing peer the companion has an open EA / RFCOMM session with. */
export type BridgethingSessionPeer = {
  id: string;
  /** BT-advertised name. Often identical across multiple Car Things. */
  name: string;
};

/** A music-provider glue installed in the host app. */
export type BridgethingProviderInfo = {
  /** Stable id (e.g. "spotify"); used for `setActiveProvider`. */
  id: string;
  /** Human-readable label for UI. */
  displayName: string;
  /** True if the glue's `attach` is implemented; false for "coming soon" stubs. */
  available: boolean;
};

export type BridgethingAuthKind = 'idle' | 'pending' | 'authenticated' | 'failed';

/**
 * Auth lifecycle snapshot the active glue surfaces.
 *
 * - `kind === "pending"` may carry device-code prompt data: `userCode`,
 *   `verificationUrl`, and `verificationUrlComplete`. JS opens the
 *   prefilled URL in InAppBrowser.
 * - `kind === "failed"` carries `message`.
 *
 * Other kinds leave the optional fields undefined.
 *
 * Flat-shaped instead of a discriminated union because Nitro's C++ codegen
 * can't represent anonymous tagged unions.
 */
export type BridgethingAuthState = {
  kind: BridgethingAuthKind;
  userCode?: string;
  verificationUrl?: string;
  verificationUrlComplete?: string;
  message?: string;
};

export type BridgethingRepeatMode = 'off' | 'one' | 'all';

/** Snapshot of the currently-playing track. All fields optional; `null` track = nothing playing. */
export type BridgethingNowPlayingTrack = {
  id?: string;
  title?: string;
  artist?: string;
  album?: string;
  /** Raw https URL; bypasses the asset cache so RN's Image loads directly from the provider's CDN. */
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
  /** Display name of the source app, e.g. "Spotify". */
  appName?: string;
};

/**
 * Daemon-observed ANCS authorization state.
 *
 * - `unknown`: no iAP2 link yet, or daemon hasn't probed ANCS.
 * - `authorized`: iPhone accepted notifications-sharing; ANCS attribute fetches are succeeding.
 * - `unauthorized`: ANCS is exposed but content reads are rejected (no LE bond, or user declined).
 */
export type BridgethingAncsAuthStatus = 'unknown' | 'probing' | 'authorized' | 'unauthorized';

/** Outcome of `enableAncsNotifications`'s setup-flow attempt. */
export type BridgethingAncsSetupKind = 'paired' | 'alreadyPaired' | 'cancelled' | 'unsupported' | 'failed';

/**
 * Result of the AccessorySetupKit pair flow. `kind` is the picker-side outcome;
 * `authStatus` is the daemon-reported state at return (may still transition;
 * observe `setOnAncsAuthStatusChanged` for the final word).
 *
 * - `"paired"`: user picked the accessory; ASK paired LE.
 * - `"alreadyPaired"`: accessory already in `ASAccessorySession.accessories`; no picker shown.
 * - `"cancelled"`: user dismissed the picker.
 * - `"unsupported"`: not iOS 18+, or no AccessorySetupKit in this build.
 * - `"failed"`: ASK or CoreBluetooth error; `message` carries it.
 */
export type BridgethingAncsSetupResult = {
  kind: BridgethingAncsSetupKind;
  authStatus: BridgethingAncsAuthStatus;
  message?: string;
};

/** "builtin" bundles ship inside the daemon and cannot be uninstalled. */
export type BridgethingWebappSource = 'builtin' | 'installed';

/** "launcher" bundles are hidden from user-facing listings. The
 *  dashboard filters these out by default. */
export type BridgethingWebappRole = 'standard' | 'launcher';

/**
 * Subset of the wire-protocol `WebappInfo` in RN-friendly shapes. UUIDs are canonical
 * hyphenated strings; the adjacent-tagged ConfigField enum is flattened into `BridgethingConfigField`.
 */
export type BridgethingWebappInfo = {
  /** UUIDv7 string baked into the bundle at scaffold time. */
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

/** The currently active webapp on the connected device, if any. */
export type BridgethingActiveWebapp = {
  id: string;
  name?: string;
};

export type BridgethingConfigKind = 'string' | 'number' | 'boolean' | 'enum' | 'secret';

/**
 * One declared user-tunable setting on a webapp. The wire enum is
 * adjacent-tagged (`{type, data}`); this RN shape is flat with `kind`
 * + per-kind optional fields, picked up by the per-app config editor.
 */
export type BridgethingConfigField = {
  kind: BridgethingConfigKind;
  key: string;
  label: string;
  /** string / secret only */
  pattern?: string;
  /** string / secret only */
  minLength?: number;
  /** string / secret only */
  maxLength?: number;
  /** number only */
  min?: number;
  /** number only */
  max?: number;
  /** number only */
  step?: number;
  /** enum only */
  choices?: string[];
  /** All kinds. Stored as the same string shape KV uses on the wire:
   *  number => decimal, boolean => "true" / "false", string/enum/secret => as-is. */
  defaultValue?: string;
};

/** One stored config row as the daemon hands it back from `configList`. */
export type BridgethingConfigEntry = {
  key: string;
  /** Stored as a string. Parse per the matching field's `kind`. */
  value: string;
};

/** Decoded webapp icon written to a temp file; hand `fileUri` directly to RN's Image. */
export type BridgethingWebappIcon = {
  /** `file://...` path to the written PNG/JPEG. Native re-uses the same filename on re-fetch;
   *  consumers diffing on uri should bust their cache when the icon may have changed. */
  fileUri: string;
  mime?: string;
};

/**
 * Companion-side capability flags. `SurfaceAvailability` is announced to the daemon on each
 * connect; flipping a flag triggers a re-announce.
 *
 * - `geo`: forwarded CoreLocation fixes.
 * - `notifications`: ANCS bridging (read-only iPhone notifications).
 * - `netFetch`: gateway-side https fetch for webapps.
 * - `netWs`: gateway-side websocket proxy for webapps.
 * - `audioTts`: TTS earcon synthesis; off by default.
 */
export type BridgethingCapabilityFlags = {
  geo: boolean;
  notifications: boolean;
  netFetch: boolean;
  netWs: boolean;
  audioTts: boolean;
};

/** OTA manifest poll configuration. JS persists this in mmkv and reapplies on bootstrap. */
export type BridgethingOtaPollConfig = {
  /** Channel selected by the user (e.g. "stable" or "dev"). Mismatches with the device's
   *  announced channel emit `channelMismatch` instead of pushing. */
  channel: string;
  /** Seconds between polls. Floor of 60 enforced by the service. */
  intervalSeconds: number;
  /** When false, only `updateAvailable` is emitted; the host app drives the push via `pollOtaNow()`. */
  autoPush: boolean;
  /** Override only for self-hosting; defaults to https://ota.bridgething.com */
  rootUrl?: string;
};

export type BridgethingOtaKind = 'image' | 'daemon';

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

/**
 * Translated `OtaPollEvent` from the companion's OtaService. `kind`
 * discriminates; per-kind fields populate as documented. Phase-level
 * progress arrives as `kind: "progress"` with `phase` + `percent`.
 */
export type BridgethingOtaEvent = {
  kind: BridgethingOtaEventKind;
  /** manifestPolled. */
  updatedAt?: string;
  /** manifestPollFailed, channelMismatch, failed. */
  reason?: string;
  /** channelMismatch, updateAvailable, progress, updated, failed. */
  deviceId?: string;
  /** updateAvailable, progress, updated, failed. */
  otaKind?: BridgethingOtaKind;
  /** updateAvailable. */
  fromVersion?: string;
  /** updateAvailable, updated. */
  toVersion?: string;
  /** progress. */
  phase?: BridgethingOtaPhase;
  /** progress. */
  percent?: number;
  /** channelMismatch. */
  deviceChannel?: string;
  /** channelMismatch. */
  configuredChannel?: string;
};

/** Bond state returned by `presentPairPicker`. Always `bonded` on success; typed for future expansion. */
export type BridgethingBtBondState = 'none' | 'bonding' | 'bonded';

/**
 * Car Thing returned by the OS pair picker (AccessorySetupKit / CompanionDeviceManager).
 * `address` is the BT MAC on Android and an opaque identifier on iOS; treat it as opaque.
 */
export type BridgethingBtDevice = {
  address: string;
  name?: string;
  bondState: BridgethingBtBondState;
  isCarThing: boolean;
};

/** Subset of the wire-protocol `BridgeThingMeta` exposed for UI. */
export type BridgethingDeviceMeta = {
  daemonVersion: string;
  appName: string;
  osName: string;
  osVersion: string;
  channel: string;
  modelName: string;
  serialNumber: string;
};

/** Companion-side identity; mirrors what is announced to the daemon as `GatewayInfo`. */
export type BridgethingHostInfo = {
  /** Display name (e.g. "bridgething"). Matches `CFBundleName`. */
  appName: string;
  /** Marketing version of the host app, read from `CFBundleShortVersionString`. */
  appVersion: string;
  /** "iOS" or "Android". */
  osName: string;
  /** OS version string (e.g. "26.0"). */
  osVersion: string;
  /** Stable per-vendor identifier (`identifierForVendor` on iOS). Empty if not exposed. */
  hostIdentifier: string;
  /** Version of the BridgethingCompanion Swift / Kotlin package. */
  libVersion: string;
  /** Version of the wire-protocol crate (libbridgething). */
  libbridgethingVersion: string;
  /** Transport-adapter label (e.g. "eaccessory" on iOS, "rfcomm" on Android). */
  adapterVersion: string;
};

/**
 * Native session module bridging the RN shell to the Swift/Kotlin `BridgethingCompanion`.
 * Native owns the gateway, iAP2/RFCOMM transport, active glue, and every dispatcher.
 * RN owns settings UI and observes state via the callback setters below.
 *
 * Provider selection: glues are registered at native startup (one factory per provider id);
 * RN calls `setActiveProvider(id)` and native instantiates the glue from the registry.
 *
 * Webapp/OTA methods take a `deviceId` from `connectedPeers()` and throw `noPeerConnected`
 * if it's no longer in the connected set.
 *
 * Storage: native persists tokens through provider-owned Keychain paths. Everything else
 * (capability flags, OTA poll config) lives in mmkv on the JS side and is re-applied on bootstrap.
 */
export interface BridgethingSession extends HybridObject<{ ios: 'swift'; android: 'kotlin' }> {
  /** Open the iAP2/RFCOMM adapter, start every dispatcher, and announce capabilities. */
  start(): Promise<void>;

  /** Tear down the gateway, dispatchers, and active glue (if any). */
  stop(): Promise<void>;

  /** All glues registered with this build of the host app. */
  availableProviders(): Promise<BridgethingProviderInfo[]>;

  /**
   * Detach the current glue and attach the one identified by `id`. Pass `null` to detach only.
   * Throws if `id` is unknown or the glue's `attach` failed.
   */
  setActiveProvider(id: string | null): Promise<void>;

  /** The currently-attached glue's info, or null if none. */
  currentProvider(): Promise<BridgethingProviderInfo | null>;

  /** Abort an in-flight auth attempt. No-op if no auth is pending. */
  cancelAuth(): Promise<void>;

  /**
   * Clear persisted auth state (Keychain on iOS, EncryptedSharedPreferences on Android),
   * detach the glue, and emit `authStateChanged({ kind: "idle" })`. Used by Settings -> Sign out.
   */
  signOut(): Promise<void>;

  /** Snapshot of currently connected Car Thing peers. */
  connectedPeers(): Promise<BridgethingSessionPeer[]>;

  /** Latest NowPlaying snapshot, or null if none ever received this session. */
  currentNowPlaying(): Promise<BridgethingNowPlaying | null>;

  /**
   * Drive the iOS AccessorySetupKit pair flow that creates the LE bond ANCS requires.
   * Returns once the picker resolves; ANCS auth state may transition asynchronously -
   * observe `setOnAncsAuthStatusChanged`. iOS 18+ only; earlier iOS and Android resolve
   * as `kind: "unsupported"`.
   */
  enableAncsNotifications(): Promise<BridgethingAncsSetupResult>;

  /** Latest daemon-reported ANCS auth state. `unknown` until the daemon emits one. */
  ancsAuthStatus(): Promise<BridgethingAncsAuthStatus>;

  /** Installed bundles on `deviceId`, excluding `launcher` role bundles. */
  listWebapps(deviceId: string): Promise<BridgethingWebappInfo[]>;

  /** The currently-active bundle on `deviceId`, or null. */
  currentWebapp(deviceId: string): Promise<BridgethingActiveWebapp | null>;

  /**
   * Install a `.zip` bundle already downloaded by JS. Pass the archive as base64 (no `data:` prefix);
   * the daemon validates `manifest.json` and rejects if invalid. Call per deviceId for multi-device.
   *
   * Base64 rather than `ArrayBuffer` because the Swift `ArrayBuffer` typealias breaks
   * Swift-to-C++ header interop on iOS; 33% inflation is acceptable for webapp bundle sizes.
   */
  installWebappFromBase64(deviceId: string, archiveBase64: string): Promise<BridgethingWebappInfo>;

  /** Uninstall the bundle from `deviceId`. Builtin bundles cannot be uninstalled. */
  uninstallWebapp(deviceId: string, id: string): Promise<void>;

  /** Set the active bundle on `deviceId`. Triggers a daemon-side kiosk reload. */
  switchWebapp(deviceId: string, id: string): Promise<void>;

  /**
   * Fetch the bundle's icon from `deviceId` and write it to a temp file. Returns a `file://` URI
   * for RN's `Image`, or `null` if the manifest declares no icon. The same temp file is reused
   * per `(deviceId, id)` so the URI stays stable.
   */
  webappIcon(deviceId: string, id: string): Promise<BridgethingWebappIcon | null>;

  /** All stored config entries for a webapp on `deviceId`, default-seeded at install. */
  listWebappConfig(deviceId: string, id: string): Promise<BridgethingConfigEntry[]>;

  /**
   * Write one config field on `deviceId`. `value` is the storage-format string;
   * encode per the field's `kind` (number => decimal, boolean => "true"/"false",
   * string/enum/secret => as-is). Daemon validates against the manifest schema.
   */
  setWebappConfigField(deviceId: string, id: string, key: string, value: string): Promise<void>;

  /**
   * Reset one config field to its manifest default (or delete if no default declared).
   * Daemon broadcasts `Changed` to the active webapp on `deviceId`.
   */
  deleteWebappConfigField(deviceId: string, id: string, key: string): Promise<void>;

  /** Apply capability flags and re-announce to the daemon. JS owns the mmkv copy and calls this on bootstrap and change. */
  setCapabilityFlags(flags: BridgethingCapabilityFlags): Promise<void>;

  /**
   * Set or replace the manifest poll configuration. Pass `null` to disable polling.
   * JS owns the mmkv copy and re-applies on bootstrap.
   */
  setOtaPollConfig(config: BridgethingOtaPollConfig | null): Promise<void>;

  /** Run one poll iteration immediately, regardless of the interval timer. */
  pollOtaNow(): Promise<void>;

  /** Latest `BridgeThingMeta` the daemon announced for `deviceId`, or null if not yet seen. */
  deviceMeta(deviceId: string): Promise<BridgethingDeviceMeta | null>;

  /** Companion-side identity snapshot. Used by Settings -> About. */
  hostInfo(): Promise<BridgethingHostInfo>;

  /**
   * OS-mediated pair flow: AccessorySetupKit (iOS 18+) or CompanionDeviceManager (Android API 26+).
   * Returns the chosen accessory, or `null` on cancel.
   */
  presentPairPicker(): Promise<BridgethingBtDevice | null>;

  /** True when the app is toggled on in Android's "Device & app notifications" settings.
   *  iOS uses ANCS instead and always returns false here. */
  isNotificationAccessGranted(): Promise<boolean>;

  /** Open Android's "Notification access" settings page. iOS rejects with `unsupported`. */
  requestNotificationAccess(): Promise<void>;

  /**
   * Drop runtime permissions. Android 13+ (API 33+) uses `Context.revokeSelfPermissionsOnKill`
   * to queue the revoke for the next process kill; returns `true`. Android <=12 and iOS return
   * `false`; callers fall back to opening app settings.
   *
   * Pass `android.permission.*` constants. Revoking only the background variant downgrades to
   * "while using" - pass all related permissions together for a full revoke.
   */
  revokeRuntimePermissions(permissions: string[]): Promise<boolean>;

  /**
   * Force-kill our own process to apply a queued `revokeSelfPermissionsOnKill`.
   * Android-only; iOS rejects.
   */
  killApp(): Promise<void>;

  setOnProviderChanged(callback: (info: BridgethingProviderInfo | null) => void): void;
  setOnAuthStateChanged(callback: (state: BridgethingAuthState) => void): void;
  setOnPeerConnected(callback: (peer: BridgethingSessionPeer) => void): void;
  setOnPeerDisconnected(callback: (peerId: string) => void): void;
  setOnNowPlayingChanged(callback: (now: BridgethingNowPlaying | null) => void): void;
  setOnAncsAuthStatusChanged(callback: (status: BridgethingAncsAuthStatus) => void): void;
  setOnLog(callback: (level: string, message: string) => void): void;
  /** Toggle the underlying log stream. Off by default; enable only while a UI actively renders
   *  log lines. Every line crosses JSI, so unconditional streaming has real cost on a 512MB device.
   *  The JS session refcounts subscribers and flips this automatically. */
  setLogStreamingEnabled(enabled: boolean): void;

  /** Fires after install/uninstall/switch on `deviceId`. Active-webapp changes from the
   *  stock webapp or daemon fallback are not surfaced here. */
  setOnWebappsChanged(callback: (deviceId: string) => void): void;
  /** Live per-device meta updates (on connect, after OTA, on reannounce). */
  setOnDeviceMetaChanged(callback: (deviceId: string, meta: BridgethingDeviceMeta) => void): void;
  /** Stream of OtaPollEvent translated to RN shape. */
  setOnOtaEvent(callback: (event: BridgethingOtaEvent) => void): void;
}
