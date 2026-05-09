import type { HybridObject } from 'react-native-nitro-modules';

/** Connected Car Thing peer the companion has an open EA / RFCOMM session with. */
export type BridgethingSessionPeer = {
  id: string;
  /** BT-advertised name. Often identical across multiple Car Things. */
  name: string;
  /** User-assigned local nickname, persisted in UserDefaults. UI prefers
   *  this over `name` when set. Set/cleared via `setDeviceNickname`. */
  nickname?: string;
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
 *   `verificationUrl`, and `verificationUrlComplete` (the prefilled URL
 *   the native side opens in SFSafariViewController so the user lands
 *   on the page with their code already filled).
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

/** Snapshot of the currently-playing track. All fields optional - the
 *  active glue may not know all of them. `null` track = nothing playing. */
export type BridgethingNowPlayingTrack = {
  id?: string;
  title?: string;
  artist?: string;
  album?: string;
  /** Raw https URL (bypasses the asset-cache indirection so RN's Image
   *  component can load directly from the provider's CDN). */
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
 * Daemon-observed ANCS authorization state. Mirrors the wire-protocol
 * `AncsAuthState` enum (see `crates/lib/src/shared/notifications.rs`).
 *
 * - `unknown`: no iAP2 link yet, or daemon hasn't probed ANCS yet.
 * - `authorized`: the iPhone has accepted notifications-sharing for this
 *   peer and ANCS attribute fetches are succeeding.
 * - `unauthorized`: the iPhone exposed ANCS but rejects content reads
 *   (no LE bond, or user declined the notifications-sharing prompt).
 */
export type BridgethingAncsAuthStatus = 'unknown' | 'probing' | 'authorized' | 'unauthorized';

/** Outcome of `enableAncsNotifications`'s setup-flow attempt. */
export type BridgethingAncsSetupKind = 'paired' | 'alreadyPaired' | 'cancelled' | 'unsupported' | 'failed';

/**
 * Result of triggering the AccessorySetupKit pair flow. `kind` describes
 * the picker-side outcome; `authStatus` is the daemon-reported state at
 * return time (may still transition shortly after — listen on
 * `setOnAncsAuthStatusChanged` for the final word).
 *
 * - `kind === "paired"`: user picked the accessory and ASK paired LE.
 * - `kind === "alreadyPaired"`: the accessory was already in
 *   `ASAccessorySession.accessories`; no picker shown.
 * - `kind === "cancelled"`: user dismissed the picker.
 * - `kind === "unsupported"`: not iOS 18+, or the device this build
 *   targets has no AccessorySetupKit.
 * - `kind === "failed"`: ASK or CoreBluetooth surfaced an error;
 *   `message` carries it.
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
 * Subset of the wire-protocol `WebappInfo` projected into RN-friendly
 * shapes. UUIDs come across as canonical hyphenated strings; the
 * adjacent-tagged ConfigField enum is flattened into
 * `BridgethingConfigField`.
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

/** Bytes + mime returned by `webappIcon`. The base64 string excludes
 *  the `data:` prefix; the consumer wraps it as `data:<mime>;base64,...` */
export type BridgethingWebappIcon = {
  base64: string;
  mime?: string;
};

/**
 * Companion-side capability flags. The composed `SurfaceAvailability`
 * is announced to the daemon on each connect; flipping a flag here
 * triggers a re-announce.
 *
 * - `geo`: forwarded location fixes from CoreLocation.
 * - `notifications`: ANCS bridging (read-only iPhone notifications).
 * - `netFetch`: gateway-side https fetch on behalf of webapps.
 * - `netWs`: gateway-side websocket proxy on behalf of webapps.
 * - `audioTts`: TTS earcon synthesis (off by default; phone-side voice).
 */
export type BridgethingCapabilityFlags = {
  geo: boolean;
  notifications: boolean;
  netFetch: boolean;
  netWs: boolean;
  audioTts: boolean;
};

/**
 * Configuration for the OTA manifest poll loop. Persisted via
 * UserDefaults; `setOtaPollConfig(null)` disables polling.
 */
export type BridgethingOtaPollConfig = {
  /** Channel the user has selected (e.g. "stable" or "dev"). Channel
   *  mismatches with the device's announced channel emit a
   *  `channelMismatch` event instead of pushing. */
  channel: string;
  /** Seconds between polls. Floor of 60 enforced by the service. */
  intervalSeconds: number;
  /** When false, only `updateAvailable` is emitted; the host app drives
   *  the push via `pollOtaNow()` followed by a manual prompt. */
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

/**
 * Native session module bridging the React Native UI shell to the Swift /
 * Kotlin `BridgethingCompanion`. The native side owns the gateway, the
 * iAP2 / RFCOMM transport, the active glue, and every dispatcher (Player,
 * Asset, Lyrics, Net, Geo, Volume). RN holds settings UI and observes
 * state through the callback setters below.
 *
 * Provider selection model: glues are registered by the host app's native
 * entry point at startup (one factory closure per provider id). RN calls
 * `setActiveProvider(id)` to switch - native instantiates the glue from
 * the registry and hands it to the companion.
 *
 * Webapp / OTA methods that touch a specific Car Thing take a
 * `deviceId` argument matching one of the ids returned by
 * `connectedPeers()`. Methods throw `noPeerConnected` if the deviceId
 * isn't currently in the connected set.
 */
export interface BridgethingSession extends HybridObject<{ ios: 'swift'; android: 'kotlin' }> {
  /**
   * Bring the gateway up: open the iAP2 / RFCOMM adapter, start every
   * dispatcher, and announce capabilities (no provider yet).
   */
  start(): Promise<void>;

  /** Tear down the gateway, dispatchers, and active glue (if any). */
  stop(): Promise<void>;

  /** All glues registered with this build of the host app. */
  availableProviders(): Promise<BridgethingProviderInfo[]>;

  /**
   * Detach the current glue and attach the one identified by `id`. Pass
   * `null` to detach without replacing. Throws if `id` is unknown or the
   * glue's `attach` (auth + dealer connect) failed.
   */
  setActiveProvider(id: string | null): Promise<void>;

  /** The currently-attached glue's info, or null if none. */
  currentProvider(): Promise<BridgethingProviderInfo | null>;

  /**
   * Abort an in-flight auth attempt. No-op if no auth is pending. Used
   * when the user backs out of the device-code pairing screen.
   */
  cancelAuth(): Promise<void>;

  /**
   * Clear persisted auth state for the active provider (Keychain on iOS,
   * EncryptedSharedPreferences on Android), detach the glue, and emit
   * `authStateChanged({ kind: "idle" })`. Used by Settings → Sign out.
   */
  signOut(): Promise<void>;

  /** Snapshot of currently connected Car Thing peers. */
  connectedPeers(): Promise<BridgethingSessionPeer[]>;

  /** Latest NowPlaying snapshot, or null if none ever received this session. */
  currentNowPlaying(): Promise<BridgethingNowPlaying | null>;

  /**
   * Drive the iOS AccessorySetupKit pair flow that creates the LE bond
   * the daemon needs before ANCS will be exposed. Returns once the
   * picker-side flow resolves; the actual ANCS authorization state may
   * land asynchronously over the daemon's wire surface — observe
   * `setOnAncsAuthStatusChanged` for the final word. iOS 18+ only;
   * earlier iOS versions resolve as `kind: "unsupported"`. Android
   * resolves as `unsupported` immediately.
   */
  enableAncsNotifications(): Promise<BridgethingAncsSetupResult>;

  /** Latest daemon-reported ANCS auth state. `unknown` until the daemon emits one. */
  ancsAuthStatus(): Promise<BridgethingAncsAuthStatus>;

  // MARK: - Device naming

  /** Set or clear (`null`) a local nickname for the device. The
   *  companion stores it in UserDefaults and re-emits the affected
   *  peer through `setOnPeerConnected` so the UI re-renders. */
  setDeviceNickname(deviceId: string, nickname: string | null): Promise<void>;

  /** Read the nickname for `deviceId`, or `null` if none set. */
  getDeviceNickname(deviceId: string): Promise<string | null>;

  // MARK: - Webapps (per-device)

  /** Installed bundles on `deviceId`. Filters out `launcher` role
   *  bundles by default; the dashboard renders the rest. */
  listWebapps(deviceId: string): Promise<BridgethingWebappInfo[]>;

  /** The currently-active bundle on `deviceId`, or null. */
  currentWebapp(deviceId: string): Promise<BridgethingActiveWebapp | null>;

  /**
   * Download a `.zip` bundle from `url` and install it on `deviceId`.
   * The bundle's `manifest.json` is validated daemon-side. Throws on
   * download failure or daemon-side install rejection. To install on
   * multiple devices, call this method per deviceId.
   */
  installWebappFromUrl(deviceId: string, url: string): Promise<BridgethingWebappInfo>;

  /** Uninstall the bundle from `deviceId`. Builtin bundles cannot
   *  be uninstalled. */
  uninstallWebapp(deviceId: string, id: string): Promise<void>;

  /** Set the active bundle on `deviceId`. Triggers a kiosk reload
   *  daemon-side. */
  switchWebapp(deviceId: string, id: string): Promise<void>;

  /**
   * Fetch the bundle's icon from `deviceId` as a base64 data string.
   * The webapp's detail view embeds it as `data:<mime>;base64,<base64>`.
   * Returns `null` if the manifest doesn't declare an icon.
   */
  webappIcon(deviceId: string, id: string): Promise<BridgethingWebappIcon | null>;

  /** All stored config entries for a webapp on `deviceId`
   *  (default-seeded at install). */
  listWebappConfig(deviceId: string, id: string): Promise<BridgethingConfigEntry[]>;

  /**
   * Write one config field on `deviceId`. `value` is the storage-format
   * string the KV layer holds; the editor encodes per the field's
   * `kind` (number => decimal, boolean => "true"/"false",
   * string/enum/secret => as-is). Daemon validates against the
   * manifest schema.
   */
  setWebappConfigField(deviceId: string, id: string, key: string, value: string): Promise<void>;

  /**
   * Reset one config field on `deviceId` to the manifest default
   * (or delete if no default declared). Daemon emits a `Changed`
   * broadcast to the active webapp if it's the one being edited.
   */
  deleteWebappConfigField(deviceId: string, id: string, key: string): Promise<void>;

  // MARK: - Capability flags

  /** Current companion-side capability flags. Persisted via UserDefaults. */
  getCapabilityFlags(): Promise<BridgethingCapabilityFlags>;

  /** Replace capability flags and re-announce capabilities to the daemon. */
  setCapabilityFlags(flags: BridgethingCapabilityFlags): Promise<void>;

  // MARK: - OTA

  /**
   * Set or replace the manifest poll configuration. Pass `null` to
   * disable polling (manual `pollOtaNow()` and inbound range serving
   * still work). Persisted via UserDefaults; restored on next start.
   */
  setOtaPollConfig(config: BridgethingOtaPollConfig | null): Promise<void>;

  /** Read back the current poll configuration. */
  getOtaPollConfig(): Promise<BridgethingOtaPollConfig | null>;

  /** Run one poll iteration immediately, regardless of where the
   *  interval timer is. Useful when the user taps "Check for updates". */
  pollOtaNow(): Promise<void>;

  /** Latest BridgeThingMeta the daemon announced for `deviceId`, or
   *  null if none has been seen yet. Live updates arrive via
   *  `setOnDeviceMetaChanged`. */
  deviceMeta(deviceId: string): Promise<BridgethingDeviceMeta | null>;

  // MARK: - Callbacks

  setOnProviderChanged(callback: (info: BridgethingProviderInfo | null) => void): void;
  setOnAuthStateChanged(callback: (state: BridgethingAuthState) => void): void;
  setOnPeerConnected(callback: (peer: BridgethingSessionPeer) => void): void;
  setOnPeerDisconnected(callback: (peerId: string) => void): void;
  setOnNowPlayingChanged(callback: (now: BridgethingNowPlaying | null) => void): void;
  setOnAncsAuthStatusChanged(callback: (status: BridgethingAncsAuthStatus) => void): void;
  setOnLog(callback: (level: string, message: string) => void): void;
  /** Fires after any local action that mutates a device's webapp
   *  registry (install / uninstall / switch). The deviceId identifies
   *  which device's list to refresh. Active-webapp changes from
   *  elsewhere (the stock webapp, daemon-side fallback) are not
   *  surfaced today. */
  setOnWebappsChanged(callback: (deviceId: string) => void): void;
  /** Live per-device meta updates. Fires when a Car Thing announces
   *  its `BridgeThingMeta` (on connect, after OTA, on reannounce). */
  setOnDeviceMetaChanged(callback: (deviceId: string, meta: BridgethingDeviceMeta) => void): void;
  /** Stream of OtaPollEvent translated to RN shape. */
  setOnOtaEvent(callback: (event: BridgethingOtaEvent) => void): void;
}
