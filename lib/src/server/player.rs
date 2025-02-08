use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Album, Artist, PlaybackOptions, PlaybackQueue, PlaybackRestrictions, Track, CARTHING_HACKS_LOGO};

// TODO: refactor this into more command types so not spotify-specific

#[serde_with::skip_serializing_none]
#[derive(derive_more::Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(tag = "action", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "server.ts")]
pub enum ServerPlayerEvent {
  PlayerIdle,
  PlayerState {
    context_id: String,
    context_title: String,
    is_paused: bool,
    playback_options: PlaybackOptions,
    playback_position: usize,
    playback_restrictions: PlaybackRestrictions,
    playback_speed: f64,
    track: Track,
  },
  Queue {
    next: Vec<Track>,
    current: Track,
    previous: Vec<Track>,
  },

  Image {
    id: String,
    height: usize,
    width: usize,
    #[debug(skip)]
    data: String,
  },
}

impl ServerPlayerEvent {
  // TODO: remove testing code
  pub fn dummy() -> Self {
    let artist = Artist {
      name: "Thing Labs".to_string(),
      id: "bridgething:artist:bridgething".to_string(),
    };
    Self::PlayerState {
      context_id: "bridgething:context:fake".to_string(),
      context_title: "BridgeThing".to_string(),
      is_paused: false,
      playback_options: PlaybackOptions {
        repeat: 0,
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

  // TODO: remove testing code
  pub fn dummy_queue() -> Self {
    let ServerPlayerEvent::PlayerState { track, .. } = ServerPlayerEvent::dummy() else {
      panic!("infallible test code");
    };

    Self::Queue {
      next: vec![],
      current: track,
      previous: vec![],
    }
  }

  // TODO: remove testing code
  pub fn dummy_img(size: usize) -> Self {
    Self::Image {
      id: "spotify:image:bridgething".to_string(),
      height: size,
      width: size,
      data: CARTHING_HACKS_LOGO.to_owned(),
    }
  }
}

impl From<PlaybackQueue> for ServerPlayerEvent {
  fn from(queue: PlaybackQueue) -> Self {
    Self::Queue {
      next: queue.next,
      current: queue.current,
      previous: queue.previous,
    }
  }
}
