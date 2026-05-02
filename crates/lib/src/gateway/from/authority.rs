use bridgething_macros::BridgeEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

/// Surfaces the companion can declare authority over. Each scope has a
/// scope-defined fallback source the daemon merges with when no claim is
/// active or the claim has gone stale (see daemon-side merge in
/// `core::player`). New scopes are forward-compat: an unknown scope
/// arriving at an older daemon is stored opaquely and ignored.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum CompanionAuthorityScope {
  /// Track metadata: title, album, artist, persistent_id, artwork_id,
  /// duration, liked. Excludes playback state. Companion claims when it
  /// has rich app-specific data for the currently-playing iOS app;
  /// releases when the user switches to a non-tracked app.
  NowPlayingMetadata,
  /// Playback state: position, playing/paused, shuffle, repeat,
  /// app_bundle, app_display_name. On iOS, iAP2 is usually the better
  /// source (microsecond-fresh playhead). On Android the companion is
  /// the only source and always claims this.
  NowPlayingPlayback,
}

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct AuthorityClaim {
  pub scope: CompanionAuthorityScope,
}

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct AuthorityRelease {
  pub scope: CompanionAuthorityScope,
}

/// Companion declares per-scope authority. `Claim` is idempotent and may
/// be re-issued to refresh the freshness timestamp. `Release` is the
/// "stop preferring my data for this scope" signal. Stale claims fall
/// back automatically after `AUTHORITY_STALE_TIMEOUT_SECS` (default 5).
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::GatewayToBridgeMsgData)]
pub enum GatewayToBridgeAuthorityMsg {
  #[bridge_event]
  Claim(AuthorityClaim),
  #[bridge_event]
  Release(AuthorityRelease),
}
