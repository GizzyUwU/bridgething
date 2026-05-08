import type { HybridObject } from 'react-native-nitro-modules';

/** Connected Car Thing peer the companion has an open EA / RFCOMM session with. */
export type BridgethingSessionPeer = {
  id: string;
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

  setOnProviderChanged(callback: (info: BridgethingProviderInfo | null) => void): void;
  setOnAuthStateChanged(callback: (state: BridgethingAuthState) => void): void;
  setOnPeerConnected(callback: (peer: BridgethingSessionPeer) => void): void;
  setOnPeerDisconnected(callback: (peerId: string) => void): void;
  setOnNowPlayingChanged(callback: (now: BridgethingNowPlaying | null) => void): void;
  setOnAncsAuthStatusChanged(callback: (status: BridgethingAncsAuthStatus) => void): void;
  setOnLog(callback: (level: string, message: string) => void): void;
}
