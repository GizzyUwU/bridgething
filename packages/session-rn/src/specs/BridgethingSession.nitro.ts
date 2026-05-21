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

/** Decoded webapp icon. The bytes have been written to a temp file the
 *  consumer can hand straight to RN's Image as `{ uri: fileUri }`. */
export type BridgethingWebappIcon = {
  /** `file://...` path to a freshly-written PNG/JPEG. The native side
   *  re-uses this filename on subsequent fetches; consumers that diff on
   *  uri may want to bust their own cache when the underlying icon could
   *  have changed. */
  fileUri: string;
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
 * Configuration for the OTA manifest poll loop. JS persists this in
 * mmkv and reapplies on bootstrap; `setOtaPollConfig(null)` disables polling.
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

/**
 * Bond state returned by `presentPairPicker`. Always `bonded` on success
 * (both iOS ASK and android CDM only resolve after the system has
 * completed the pair). Kept as a typed field for future expansion.
 */
export type BridgethingBtBondState = 'none' | 'bonding' | 'bonded';

/**
 * One Car Thing handed back by the OS-mediated pair picker
 * (AccessorySetupKit on iOS, CompanionDeviceManager on android).
 * `address` is the BT MAC on android and an opaque accessory identifier
 * on iOS - callers shouldn't interpret it beyond echoing back to the
 * wire protocol.
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

/** Companion-side identity. Mirrors what the companion announces to the
 *  daemon as `GatewayInfo`; surfaced to RN so Settings can render real
 *  values instead of hardcoded strings. */
export type BridgethingHostInfo = {
  /** Display name (e.g. "bridgething"). Matches `CFBundleName`. */
  appName: string;
  /** Marketing version of the host app, read from `CFBundleShortVersionString`. */
  appVersion: string;
  /** "iOS" or "Android". */
  osName: string;
  /** OS version string (e.g. "26.0"). */
  osVersion: string;
  /** Stable per-vendor identifier. iOS: `identifierForVendor` UUID. Empty
   *  if the platform doesn't expose one. */
  hostIdentifier: string;
  /** Version of the BridgethingCompanion Swift / Kotlin package. */
  libVersion: string;
  /** Version of the wire-protocol crate (libbridgething). */
  libbridgethingVersion: string;
  /** Transport-adapter label, e.g. "eaccessory" on iOS, "rfcomm" on
   *  macOS dev / Android. */
  adapterVersion: string;
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
 *
 * Storage scope: the native module only persists what it must — Spotify
 * tokens go through Spotiny into Keychain because Spotiny owns the auth
 * lifecycle. Everything else (setup-completed flag, device nicknames,
 * capability flags, OTA poll config) lives in JS via mmkv; the JS layer
 * re-applies flags + poll config on each bootstrap.
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

  // MARK: - Webapps (per-device)

  /** Installed bundles on `deviceId`. Filters out `launcher` role
   *  bundles by default; the dashboard renders the rest. */
  listWebapps(deviceId: string): Promise<BridgethingWebappInfo[]>;

  /** The currently-active bundle on `deviceId`, or null. */
  currentWebapp(deviceId: string): Promise<BridgethingActiveWebapp | null>;

  /**
   * Install a `.zip` bundle the JS side has already downloaded. Pass
   * the archive as a base64-encoded string (no `data:` prefix); the
   * daemon validates `manifest.json` and rejects if invalid. To install
   * on multiple devices, call this method per deviceId.
   *
   * Why base64 and not `ArrayBuffer`: Nitro 0.35.5's Swift-side
   * `ArrayBuffer` typealias breaks Swift→C++ header interop on iOS
   * for Swift HybridObjects. Webapp bundles are small (KBs–MBs) so
   * the 33% base64 inflation is acceptable.
   */
  installWebappFromBase64(deviceId: string, archiveBase64: string): Promise<BridgethingWebappInfo>;

  /** Uninstall the bundle from `deviceId`. Builtin bundles cannot
   *  be uninstalled. */
  uninstallWebapp(deviceId: string, id: string): Promise<void>;

  /** Set the active bundle on `deviceId`. Triggers a kiosk reload
   *  daemon-side. */
  switchWebapp(deviceId: string, id: string): Promise<void>;

  /**
   * Fetch the bundle's icon from `deviceId` and write it to a temp
   * file. Returns a `file://` URI suitable for RN's `Image` component.
   * Returns `null` if the manifest doesn't declare an icon. The native
   * side rewrites the same temp file on each call so the URI stays
   * stable per `(deviceId, id)`.
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

  /** Apply capability flags and re-announce capabilities to the daemon.
   *  JS owns the persisted copy in mmkv and calls this on bootstrap +
   *  every change. */
  setCapabilityFlags(flags: BridgethingCapabilityFlags): Promise<void>;

  // MARK: - OTA

  /**
   * Set or replace the manifest poll configuration. Pass `null` to
   * disable polling (manual `pollOtaNow()` and inbound range serving
   * still work). JS owns the persisted copy in mmkv and re-applies on
   * bootstrap.
   */
  setOtaPollConfig(config: BridgethingOtaPollConfig | null): Promise<void>;

  /** Run one poll iteration immediately, regardless of where the
   *  interval timer is. Useful when the user taps "Check for updates". */
  pollOtaNow(): Promise<void>;

  /** Latest BridgeThingMeta the daemon announced for `deviceId`, or
   *  null if none has been seen yet. Live updates arrive via
   *  `setOnDeviceMetaChanged`. */
  deviceMeta(deviceId: string): Promise<BridgethingDeviceMeta | null>;

  /** Companion-side identity snapshot. Used by Settings → About to
   *  render the real version instead of a hardcoded string. */
  hostInfo(): Promise<BridgethingHostInfo>;

  /**
   * Cross-platform pair flow. The OS handles scan + picker + bond in
   * a single system surface - no in-app perms, no manual list.
   *
   * - iOS: AccessorySetupKit picker (iOS 18+); rejects on earlier iOS.
   * - Android: CompanionDeviceManager picker (API 26+).
   *
   * Returns the chosen accessory, or `null` when the user cancels.
   */
  presentPairPicker(): Promise<BridgethingBtDevice | null>;

  // MARK: - Notification access (android only)

  /** True when the app has been toggled on in Android's "Device & app
   *  notifications" settings (the only way to grant access to a
   *  NotificationListenerService). iOS treats notifications differently
   *  (ANCS) and always reports false here. */
  isNotificationAccessGranted(): Promise<boolean>;

  /** Open the system "Notification access" settings page so the user
   *  can grant access manually - Android offers no programmatic prompt.
   *  iOS rejects with `unsupported`. */
  requestNotificationAccess(): Promise<void>;

  /**
   * Drop a list of runtime permissions this app currently holds. On
   * Android 13+ (API 33+) this uses `Context.revokeSelfPermissionsOnKill`,
   * which queues the revoke for the next process kill - the user
   * doesn't see a settings bounce. Returns `true` when the queued
   * revoke was scheduled, `false` when the platform offers no
   * programmatic path (Android <=12, iOS); callers fall back to
   * opening app settings.
   *
   * Pass android.permission.* constants (e.g.
   * `["android.permission.ACCESS_BACKGROUND_LOCATION",
   *   "android.permission.ACCESS_FINE_LOCATION"]`).
   *
   * Revoking only the background variant leaves the foreground grant in
   * place and the OS just downgrades to "while using"; pass every
   * related permission together when the caller wants a full revoke.
   */
  revokeRuntimePermissions(permissions: string[]): Promise<boolean>;

  /**
   * Force-kill our own process so a queued
   * `revokeSelfPermissionsOnKill` actually takes effect. The OS routes
   * users back to the launcher; they reopen bridgething and the perm
   * is gone. Android-only; iOS rejects.
   */
  killApp(): Promise<void>;

  // MARK: - Callbacks

  setOnProviderChanged(callback: (info: BridgethingProviderInfo | null) => void): void;
  setOnAuthStateChanged(callback: (state: BridgethingAuthState) => void): void;
  setOnPeerConnected(callback: (peer: BridgethingSessionPeer) => void): void;
  setOnPeerDisconnected(callback: (peerId: string) => void): void;
  setOnNowPlayingChanged(callback: (now: BridgethingNowPlaying | null) => void): void;
  setOnAncsAuthStatusChanged(callback: (status: BridgethingAncsAuthStatus) => void): void;
  setOnLog(callback: (level: string, message: string) => void): void;
  /** Toggle the underlying log stream. Off by default; turn on only
   *  while a UI is actively rendering log lines. Logs are high volume
   *  and every line crosses JSI, so unconditional streaming pulls real
   *  cost on a 512MB device. The JS session refcounts subscribers and
   *  flips this automatically. */
  setLogStreamingEnabled(enabled: boolean): void;

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
