use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
  Album, Artist, CurrentlyActiveApplication, PlaybackOptions, PlaybackRestrictions, QueueTrack, Track,
  CARTHING_HACKS_LOGO,
};

// TODO: refactor this into more command types so not spotify-specific

#[serde_with::skip_serializing_none]
#[derive(derive_more::Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(tag = "action", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "server.ts")]
pub enum ServerPlayerEvent {
  IdlePlayerState {
    context_uri: String,
    is_paused: bool,
    is_paused_bool: bool,
    playback_options: PlaybackOptions,
    playback_position: usize,
    playback_restrictions: PlaybackRestrictions,
    playback_speed: f64,
  },
  SimplePlayerState {
    currently_active_application: CurrentlyActiveApplication,
    context_uri: String,
    context_title: String,
    is_paused: bool,
    is_paused_bool: bool,
    playback_options: PlaybackOptions,
    playback_position: usize,
    playback_restrictions: PlaybackRestrictions,
    playback_speed: f64,
    track: Track,
  },
  SpotifyPlayerState {
    context_uri: String,
    context_title: String,
    is_paused: bool,
    is_paused_bool: bool,
    playback_options: PlaybackOptions,
    playback_position: usize,
    playback_restrictions: PlaybackRestrictions,
    playback_speed: f64,
    track: Track,
  },
  PlayerQueue {
    next: Vec<QueueTrack>,
    current: QueueTrack,
    previous: Vec<QueueTrack>,
  },

  Image {
    height: usize,
    width: usize,
    #[debug(skip)]
    data: String,
  },
}

impl ServerPlayerEvent {
  pub fn empty() -> Self {
    Self::IdlePlayerState {
      context_uri: "".to_string(),
      is_paused: false,
      is_paused_bool: false,
      playback_position: 0,
      playback_options: PlaybackOptions {
        repeat: 0,
        shuffle: false,
      },
      playback_restrictions: PlaybackRestrictions {
        can_repeat_context: true,
        can_repeat_track: true,
        can_seek: false,
        can_skip_next: false,
        can_skip_prev: false,
        can_toggle_shuffle: true,
      },
      playback_speed: 0.0,
    }
  }

  // TODO: remove testing code
  pub fn dummy_simple() -> Self {
    let artist = Artist {
      name: "Thing Labs".to_string(),
      uri: "spotify:artist:bridgething".to_string(),
    };
    Self::SimplePlayerState {
      currently_active_application: CurrentlyActiveApplication {
        id: "com.bridgething".to_string(),
        name: "BridgeThing".to_string(),
      },
      context_uri: "spotify:context:fake".to_string(),
      context_title: "BridgeThing".to_string(),
      is_paused: false,
      is_paused_bool: false,
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
      },
      playback_speed: 0.0,
      track: Track {
        name: "BridgeThing".to_string(),
        album: Album {
          name: "Thing Labs".to_string(),
          uri: "spotify:album:bridgething".to_string(),
        },
        artist: artist.clone(),
        artists: vec![artist],
        duration_ms: 5000,
        image_id: "spotify:image:fake:1".to_string(),
        is_episode: false,
        is_podcast: false,
        saved: true,
        uid: "bridgething".to_string(),
        uri: "spotify:context:bridgething".to_string(),
      },
    }
  }

  // TODO: remove testing code
  pub fn dummy() -> Self {
    let artist = Artist {
      name: "Thing Labs".to_string(),
      uri: "spotify:artist:bridgething".to_string(),
    };
    Self::SpotifyPlayerState {
      context_uri: "spotify:context:fake".to_string(),
      context_title: "BridgeThing".to_string(),
      is_paused: false,
      is_paused_bool: false,
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
      },
      playback_speed: 0.0,
      track: Track {
        name: "BridgeThing".to_string(),
        album: Album {
          name: "Thing Labs".to_string(),
          uri: "spotify:album:bridgething".to_string(),
        },
        artist: artist.clone(),
        artists: vec![artist],
        duration_ms: 5000,
        image_id: "spotify:image:fake:1".to_string(),
        is_episode: false,
        is_podcast: false,
        saved: true,
        uid: "bridgething".to_string(),
        uri: "spotify:context:bridgething".to_string(),
      },
    }
  }

  // TODO: remove testing code
  pub fn dummy_queue() -> Self {
    let artist = Artist {
      name: "Thing Labs".to_string(),
      uri: "spotify:artist:bridgething".to_string(),
    };

    let queue_track = QueueTrack {
      uid: "bridgething".to_string(),
      uri: "spotify:track:bridgething".to_string(),
      name: "The BridgeThing Song".to_string(),
      artists: vec![artist],
      image_uri: "spotify:image:bridgething".to_string(),
      provider: "context".to_string(),
    };

    Self::PlayerQueue {
      next: vec![],
      current: queue_track,
      previous: vec![],
    }
  }

  // TODO: remove testing code
  pub fn dummy_img(size: usize) -> Self {
    Self::Image {
      height: size,
      width: size,
      data: CARTHING_HACKS_LOGO.to_owned(),
    }
  }
}
