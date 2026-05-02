use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
  Album, Artist, CARTHING_HACKS_LOGO, PlaybackOptions, PlaybackQueue, PlaybackRestrictions, RepeatMode, Track,
};

#[serde_with::skip_serializing_none]
#[derive(derive_more::Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PlayerState {
  pub context_id: String,
  pub context_title: String,
  pub is_paused: bool,
  pub playback_options: PlaybackOptions,
  pub playback_position: usize,
  pub playback_restrictions: PlaybackRestrictions,
  pub playback_speed: f64,
  pub track: Track,
}

#[serde_with::skip_serializing_none]
#[derive(derive_more::Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PlayerQueue {
  pub next: Vec<Track>,
  pub current: Track,
  pub previous: Vec<Track>,
}

#[serde_with::skip_serializing_none]
#[derive(derive_more::Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PlayerImage {
  pub id: String,
  pub height: usize,
  pub width: usize,
  #[debug(skip)]
  pub data: String,
}

#[serde_with::skip_serializing_none]
#[derive(derive_more::Debug, Clone, Serialize, Deserialize, PartialEq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
pub enum BridgeToClientPlayerMsg {
  #[bridge_event]
  PlayerIdle,
  #[bridge_event]
  PlayerState(PlayerState),
  #[bridge_event]
  Queue(PlayerQueue),
  #[bridge_event]
  Image(PlayerImage),
}

impl PlayerState {
  pub fn dummy() -> Self {
    let artist = Artist {
      name: "Thing Labs".to_string(),
      id: "bridgething:artist:bridgething".to_string(),
    };
    Self {
      context_id: "bridgething:context:fake".to_string(),
      context_title: "BridgeThing".to_string(),
      is_paused: false,
      playback_options: PlaybackOptions {
        repeat: RepeatMode::Off,
        shuffle: false,
      },
      playback_position: 500,
      playback_restrictions: PlaybackRestrictions {
        can_repeat_context: true,
        can_repeat_track: true,
        can_seek: true,
        can_skip_next: true,
        can_skip_prev: true,
        can_toggle_shuffle: true,
        can_change_volume: true,
        can_like: true,
        can_set_output: true,
      },
      playback_speed: 0.0,
      track: Track {
        id: "dummy-bridgething-default".to_string(),
        name: "BridgeThing".to_string(),
        album: Album {
          name: "Thing Labs".to_string(),
          id: "bridgething:album:bridgething".to_string(),
        },
        artist: artist.clone(),
        artists: vec![artist],
        duration_ms: 5000,
        image_id: "bridgething:image:bridgething:image".to_string(),
        saved: true,
      },
    }
  }
}

impl BridgeToClientPlayerMsg {
  pub fn dummy_state() -> Self {
    Self::PlayerState(PlayerState::dummy())
  }

  pub fn dummy_queue() -> Self {
    let track = PlayerState::dummy().track;
    Self::Queue(PlayerQueue {
      next: vec![],
      current: track,
      previous: vec![],
    })
  }

  pub fn dummy_img(size: usize) -> Self {
    Self::Image(PlayerImage {
      id: "spotify:image:bridgething".to_string(),
      height: size,
      width: size,
      data: CARTHING_HACKS_LOGO.to_owned(),
    })
  }
}

impl From<PlaybackQueue> for BridgeToClientPlayerMsg {
  fn from(queue: PlaybackQueue) -> Self {
    Self::Queue(PlayerQueue {
      next: queue.next,
      current: queue.current,
      previous: queue.previous,
    })
  }
}
