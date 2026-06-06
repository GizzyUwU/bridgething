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
//! Every field is optional: producers send partial updates populating
//! only what changed, and unset fields keep their prior value. This
//! matches the iAP2 wire semantics (Apple emits a fresh `NowPlayingUpdate`
//! whenever any subscribed attribute changes, sending only that
//! attribute) and lets the Android companion push partial updates
//! without a full snapshot every time.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

/// `repeat` is a typed enum (Off/All/One) shared across the player
/// surface and the iAP2 NowPlaying CSM / MediaSession backends, which
/// all expose three repeat states.
#[typeshare]
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum RepeatMode {
  #[default]
  Off,
  All,
  One,
}

/// Three-state shuffle. iAP2 and Apple Music distinguish track-level
/// from album-level shuffle; companion gateways without that distinction
/// project to `Songs` when on. Webapps that just need an on/off signal
/// read `shuffle_on` (None when the underlying mode is unknown).
#[typeshare]
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum ShuffleMode {
  #[default]
  Off,
  Songs,
  Albums,
}

impl ShuffleMode {
  pub fn is_on(self) -> bool {
    !matches!(self, Self::Off)
  }
}

/// The kind of media currently playing. Multi-typed: an item can be
/// e.g. both `Podcast` and `AudioBook` (rare). Drives webapp UI choices
/// like skip-15s-vs-skip-track and chapter UI.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum MediaType {
  Music,
  Podcast,
  AudioBook,
}

/// Delta event the companion or iAP2 stream emits whenever a player
/// attribute changes. Every field is optional: producers send partial
/// updates populating only what changed, and fields left unset keep
/// their prior value.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, TS)]
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
/// to `ClientToBridgeAssetMsg::Get` to retrieve the bytes. The id namespace
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
  pub album_uri: Option<String>,
  pub album_artist: Option<String>,
  pub artist: Option<String>,
  pub artist_uri: Option<String>,
  pub liked: Option<bool>,
  pub artwork_id: Option<String>,
  pub duration_ms: Option<u32>,
  pub media_types: Option<Vec<MediaType>>,
  pub track_number: Option<u16>,
  pub track_count: Option<u16>,
  pub is_like_supported: Option<bool>,
  pub is_ban_supported: Option<bool>,
  pub is_banned: Option<bool>,
  pub is_resident_on_device: Option<bool>,
  pub chapter_count: Option<u16>,
}

impl MediaItemUpdate {
  pub fn is_empty(&self) -> bool {
    self.persistent_id.is_none()
      && self.title.is_none()
      && self.album.is_none()
      && self.album_uri.is_none()
      && self.album_artist.is_none()
      && self.artist.is_none()
      && self.artist_uri.is_none()
      && self.liked.is_none()
      && self.artwork_id.is_none()
      && self.duration_ms.is_none()
      && self.media_types.is_none()
      && self.track_number.is_none()
      && self.track_count.is_none()
      && self.is_like_supported.is_none()
      && self.is_ban_supported.is_none()
      && self.is_banned.is_none()
      && self.is_resident_on_device.is_none()
      && self.chapter_count.is_none()
  }
}

/// Per-playback-session attributes that vary regardless of track:
/// playing/paused, position, shuffle/repeat, and the iOS bundle
/// identifier of the app currently driving playback (e.g.
/// `"com.spotify.client"`). `app_bundle` is null on the Android path
/// since it isn't a meaningful surface there.
///
/// `set_elapsed_time_available` is the gate webapps must honor for
/// scrub UI: when false, scrubbing is unsupported by the foreground
/// app and the seek button must be disabled.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct PlaybackUpdate {
  pub playing: Option<bool>,
  pub position_ms: Option<u32>,
  pub shuffle: Option<bool>,
  pub shuffle_mode: Option<ShuffleMode>,
  pub repeat: Option<RepeatMode>,
  pub app_bundle: Option<String>,
  pub app_display_name: Option<String>,
  pub queue_index: Option<u32>,
  pub queue_count: Option<u32>,
  pub queue_chapter_index: Option<u32>,
  pub playback_speed: Option<f32>,
  pub set_elapsed_time_available: Option<bool>,
  pub queue_list_avail: Option<bool>,
  pub apple_music_radio_ad: Option<bool>,
  pub apple_music_radio_station_name: Option<String>,
}

impl PlaybackUpdate {
  pub fn is_empty(&self) -> bool {
    self.playing.is_none()
      && self.position_ms.is_none()
      && self.shuffle.is_none()
      && self.shuffle_mode.is_none()
      && self.repeat.is_none()
      && self.app_bundle.is_none()
      && self.app_display_name.is_none()
      && self.queue_index.is_none()
      && self.queue_count.is_none()
      && self.queue_chapter_index.is_none()
      && self.playback_speed.is_none()
      && self.set_elapsed_time_available.is_none()
      && self.queue_list_avail.is_none()
      && self.apple_music_radio_ad.is_none()
      && self.apple_music_radio_station_name.is_none()
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
