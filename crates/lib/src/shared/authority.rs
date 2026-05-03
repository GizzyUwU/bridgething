//! Authority scopes a companion can claim. Lives in shared/ so both
//! `gateway::AuthorityClaim`/`AuthorityRelease` (the inbound events that
//! mutate the daemon's authority view) and `shared::Capabilities`
//! (the outbound projection to webapps) reference one source of truth.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

/// One axis the companion can declare authority over. Each scope has a
/// fallback source the daemon merges with when no claim is active or
/// the claim has gone stale (see daemon-side merge in `core::player`).
/// Unknown scopes arriving at an older daemon are stored opaquely and
/// ignored.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
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
