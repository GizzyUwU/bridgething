//! Canonical shape for "what is the connected phone playing" data.
//!
//! Two transports populate this surface:
//!
//! - The iAP2 control session (iOS) decodes Apple's NowPlayingUpdate
//!   CSM into this shape. iOS provides this for free for any audio app
//!   that registers with `MPNowPlayingInfoCenter`; no companion app
//!   required.
//! - The bridgething gateway protocol (Android, optionally iOS
//!   companion) sends a `GatewayToBridgeMsgData::NowPlayingUpdate`
//!   carrying this struct.
//!
//! Every field is optional: producers populate only what they have
//! fresh information about, and the daemon merges into stable internal
//! state. This matches the iAP2 wire semantics (Apple emits a fresh
//! `NowPlayingUpdate` whenever any subscribed attribute changes,
//! sending only that attribute) and gives the Android companion the
//! freedom to push partial updates without a full snapshot every time.
//!
//! `repeat` is encoded as a u32 (0 = off, 1 = single track, 2 = full
//! context) to stay compatible with the existing `PlaybackOptions`
//! convention webapps already render.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct NowPlayingUpdate {
  pub media_item: Option<MediaItemUpdate>,
  pub playback: Option<PlaybackUpdate>,
}

impl NowPlayingUpdate {
  pub fn is_empty(&self) -> bool {
    let media_empty = self.media_item.as_ref().is_none_or(MediaItemUpdate::is_empty);
    let playback_empty = self.playback.as_ref().is_none_or(PlaybackUpdate::is_empty);
    media_empty && playback_empty
  }
}

/// Per-track attributes that vary per song. `persistent_id` is a stable
/// per-platform identifier (iAP2 sends u64; we hex-encode it on the
/// wire). `artwork_id` is an opaque asset id - webapps pass this value
/// to `ClientAssetCommand::Get` to retrieve the bytes. The id namespace
/// is producer-defined: iAP2 emits `iap2/art/<persistent_hex>/<n>`, the
/// companion picks whatever shape it wants (e.g. `spotify/track/<id>/image`).
/// Webapps treat the value as opaque.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct MediaItemUpdate {
  pub persistent_id: Option<String>,
  pub title: Option<String>,
  pub album: Option<String>,
  pub artist: Option<String>,
  pub liked: Option<bool>,
  pub artwork_id: Option<String>,
  pub duration_ms: Option<u32>,
}

impl MediaItemUpdate {
  pub fn is_empty(&self) -> bool {
    self.persistent_id.is_none()
      && self.title.is_none()
      && self.album.is_none()
      && self.artist.is_none()
      && self.liked.is_none()
      && self.artwork_id.is_none()
      && self.duration_ms.is_none()
  }
}

/// Per-playback-session attributes that vary regardless of track:
/// playing/paused, position, shuffle/repeat, and the iOS bundle
/// identifier of the app currently driving playback (e.g.
/// `"com.spotify.client"`). `app_bundle` is null on the Android path
/// since it isn't a meaningful surface there.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct PlaybackUpdate {
  pub playing: Option<bool>,
  pub position_ms: Option<u32>,
  pub shuffle: Option<bool>,
  pub repeat: Option<u32>,
  pub app_bundle: Option<String>,
  pub app_display_name: Option<String>,
}

impl PlaybackUpdate {
  pub fn is_empty(&self) -> bool {
    self.playing.is_none()
      && self.position_ms.is_none()
      && self.shuffle.is_none()
      && self.repeat.is_none()
      && self.app_bundle.is_none()
      && self.app_display_name.is_none()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn default_is_empty() {
    let update = NowPlayingUpdate::default();
    assert!(update.is_empty());
  }

  #[test]
  fn populated_media_item_is_not_empty() {
    let update = NowPlayingUpdate {
      media_item: Some(MediaItemUpdate {
        title: Some("Song".into()),
        ..Default::default()
      }),
      playback: None,
    };
    assert!(!update.is_empty());
  }

  #[test]
  fn empty_inner_groups_count_as_empty() {
    let update = NowPlayingUpdate {
      media_item: Some(MediaItemUpdate::default()),
      playback: Some(PlaybackUpdate::default()),
    };
    assert!(update.is_empty());
  }

  #[test]
  fn json_serialization_skips_none_fields() {
    let update = NowPlayingUpdate {
      media_item: Some(MediaItemUpdate {
        title: Some("Song".into()),
        ..Default::default()
      }),
      playback: None,
    };
    let json = serde_json::to_string(&update).unwrap();
    assert!(json.contains("\"title\":\"Song\""));
    assert!(!json.contains("artist"));
    assert!(!json.contains("playback"));
  }
}
