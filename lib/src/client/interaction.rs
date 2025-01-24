use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::stock::StockSetPreset;

// TODO: refactor this into more command types so not spotify-specific

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "action", content = "args", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub enum ClientInteractionCommand {
  // base interactions
  GetImage {
    id: String,
  },
  GetThumbnailImage {
    id: String,
  },
  GetNextTracks,
  PhoneAnswer,
  PhoneDecline,
  PhoneCallImage {
    phone_number: String,
  },
  PhoneCallMessage {
    phone_number: String,
    message: String,
  },
  IncreaseVolume,
  DecreaseVolume,
  SkipToIndex {
    index: usize,
  },
  SkipNext,
  SkipPrev {
    allow_seeking: bool,
  },
  SeekTo {
    position: usize,
  },
  Pause,
  Resume,
  SetShuffle {
    shuffle: bool,
  },
  SetRepeat {
    repeat_mode: bool,
  },

  // spotify-specific interactions - ie need a spotify sdk
  SpotifyGetChildren {
    parent_id: String,
    limit: usize,
    offset: Option<usize>,
  },
  SpotifyGetPodcast {
    uri: String,
    limit: Option<usize>,
    offset: Option<usize>,
  },
  SpotifyGetSaved {
    id: String,
  },
  SpotifyPlayPodcastTrailer {
    uri: String,
  },
  SpotifyQueueUri {
    uri: String,
  },
  SpotifySetPodcastPlaybackSpeed {
    playback_speed: usize,
  },
  SpotifySetSaved {
    id: Option<String>, // id is same as uri
    uri: Option<String>,
    saved: bool,
  },
  SpotifyPlayUri {
    uri: String,
    feature_identifier: String,
    interaction_id: Option<String>,
    skip_to_uri: Option<String>,
    skip_to_uid: Option<String>,
  },

  // legacy interactions - ie stock app only
  #[serde(rename = "spotifyGetPermissions")]
  __LegacySpotifyGetPermissions,
  #[serde(rename = "spotifySummonDj")]
  __LegacySpotifySummonDj,
  #[serde(rename = "spotifyGetHome")]
  __LegacySpotifyGetHome {
    limit: usize,
    limit_overrides: HashMap<String, usize>,
  },
  #[serde(rename = "spotifyGetPresets")]
  __LegacySpotifyGetPresets,
  #[serde(rename = "spotifySetPreset")]
  __LegacySpotifySetPreset {
    presets: Vec<StockSetPreset>,
  },
  #[serde(rename = "spotifyGetTips")]
  __LegacySpotifyGetTips,
  #[serde(rename = "spotifyGetTts")]
  __LegacySpotifyGetTts {
    file: String,
  },
}
