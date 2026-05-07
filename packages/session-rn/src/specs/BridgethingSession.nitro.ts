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
 * Auth lifecycle snapshot the active glue surfaces. `kind === "pending"`
 * may carry a `userCode` and `verificationUrl` (device-code flow);
 * `kind === "failed"` carries a `message`. Other kinds leave the optional
 * fields undefined.
 *
 * Flat-shaped instead of a discriminated union because Nitro's C++ codegen
 * can't represent anonymous tagged unions.
 */
export type BridgethingAuthState = {
  kind: BridgethingAuthKind;
  userCode?: string;
  verificationUrl?: string;
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

  /** Snapshot of currently connected Car Thing peers. */
  connectedPeers(): Promise<BridgethingSessionPeer[]>;

  setOnProviderChanged(callback: (info: BridgethingProviderInfo | null) => void): void;
  setOnAuthStateChanged(callback: (state: BridgethingAuthState) => void): void;
  setOnPeerConnected(callback: (peer: BridgethingSessionPeer) => void): void;
  setOnPeerDisconnected(callback: (peerId: string) => void): void;
  setOnLog(callback: (level: string, message: string) => void): void;
}
