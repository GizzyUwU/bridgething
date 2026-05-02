use std::collections::HashMap;

use libbridgething::{
  Album, Artist, CurrentlyActiveApplication, PlaybackOptions, PlaybackRestrictions, RepeatMode, Track,
  client::{
    BridgeToClientInteractionMsg, BridgeToClientPlayerMsg, ClientLegacyStockCommand,
    ClientToBridgeInteractionMsgCommand,
  },
  stock::StockSetPreset,
};
use serde::{Deserialize, Serialize};

use crate::handler::client::{PossibleRecvMsg, RecvMsgData};

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "method", content = "args", rename_all = "snake_case")]
pub enum StockInterAppRecv {
  #[serde(rename = "com.spotify.superbird.crashes.report")]
  CrashReport(serde_json::Value),
  #[serde(rename = "com.spotify.superbird.earcon")]
  Earcon { earcon: String }, // 'confirmation' | 'listening' | 'error'
  #[serde(rename = "com.spotify.get_available_podcast_playback_speeds")]
  GetAvailablePodcastPlaybackSpeeds {},
  #[serde(rename = "com.spotify.get_capabilities")]
  GetCapabilities {},
  #[serde(rename = "com.spotify.get_children_of_item")]
  GetChildrenOfItem {
    parent_id: String,
    limit: usize,
    offset: Option<usize>,
  },
  #[serde(rename = "com.spotify.superbird.get_home")]
  GetHome {
    limit: usize,
    limit_overrides: HashMap<String, usize>,
  },
  #[serde(rename = "com.spotify.get_crossfade_state")]
  GetCrossfadeState {},
  #[serde(rename = "com.spotify.get_current_context")]
  GetCurrentContext {},
  #[serde(rename = "com.spotify.get_current_track")]
  GetCurrentTrack {},
  #[serde(rename = "com.spotify.get_image")]
  GetImage { id: String },
  #[serde(rename = "com.spotify.get_items_for_uris")]
  GetItemForURI {},
  #[serde(rename = "com.spotify.get_next_tracks")]
  GetNextTracks {},
  #[serde(rename = "com.spotify.superbird.permissions")]
  GetPermissions {},
  #[serde(rename = "com.spotify.get_playback_speed")]
  GetPlaybackSpeed {},
  #[serde(rename = "com.spotify.superbird.player_state")]
  GetPlayerState {},
  #[serde(rename = "com.spotify.superbird.get_podcast")]
  GetPodcast {
    uri: String,
    limit: Option<usize>,
    offset: Option<usize>,
  },
  #[serde(rename = "com.spotify.get_podcast_playback_speed")]
  GetPodcastPlaybackSpeed {},
  #[serde(rename = "com.spotify.superbird.presets.get_presets")]
  GetPresets {},
  #[serde(rename = "com.spotify.get_rating")]
  GetRating {},
  #[serde(rename = "com.spotify.get_recommended_content_for_type")]
  GetRecommendedContentForType {},
  #[serde(rename = "com.spotify.get_repeat")]
  GetRepeat {},
  #[serde(rename = "com.spotify.get_root_item")]
  GetRootItem {},
  #[serde(rename = "com.spotify.get_saved")]
  GetSaved { id: String }, // id is uri
  #[serde(rename = "com.spotify.get_session_state")]
  GetSessionState {},
  #[serde(rename = "com.spotify.get_shuffle")]
  GetShuffle {},
  #[serde(rename = "com.spotify.get_thumbnail_image")]
  GetThumbnailImage { id: String },
  #[serde(rename = "com.spotify.superbird.tipsandtricks.get_tips_and_tricks")]
  GetTips {},
  #[serde(rename = "com.spotify.get_track_elapsed")]
  GetTrackElapsed {},
  #[serde(rename = "com.spotify.superbird.tts.speak")]
  GetTts { file: String },
  #[serde(rename = "com.spotify.superbird.graphql")]
  Graph,
  #[serde(rename = "com.spotify.log_message")]
  LogMessage(serde_json::Value),
  #[serde(rename = "com.spotify.superbird.pitstop.log")]
  PitstopLog(serde_json::Value),
  #[serde(rename = "com.spotify.play_item")]
  _PlayItem,
  #[serde(rename = "com.spotify.play_uri")]
  _PlayUri,
  #[serde(rename = "com.spotify.superbird.play_podcast_trailer")]
  PlayPodcastTrailer { uri: String },
  #[serde(rename = "com.spotify.queue_spotify_uri")]
  QueueUri { uri: String },
  #[serde(rename = "com.spotify.search_query")]
  SearchQuery,
  #[serde(rename = "com.spotify.set_playback_position")]
  _SeekToPosition,
  #[serde(rename = "com.spotify.set_playback_speed")]
  _SetPlaybackSpeed,
  #[serde(rename = "com.spotify.set_podcast_playback_speed")]
  SetPodcastPlaybackSpeed { playback_speed: usize },
  #[serde(rename = "com.spotify.superbird.presets.set_preset")]
  SetPreset { presets: Vec<StockSetPreset> },
  #[serde(rename = "com.spotify.set_rating")]
  SetRating,
  #[serde(rename = "com.spotify.set_repeat")]
  _SetRepeat,
  #[serde(rename = "com.spotify.set_saved")]
  SetSaved {
    id: Option<String>, // id is same as uri
    uri: Option<String>,
    saved: bool,
  },
  #[serde(rename = "com.spotify.set_shuffle")]
  _SetShuffle,
  #[serde(rename = "com.spotify.skip_next")]
  _SkipNext,
  #[serde(rename = "com.spotify.skip_previous")]
  _SkipPrevious,
  #[serde(rename = "com.spotify.skip_to_index_in_queue")]
  SkipToIndex { index: usize },
  #[serde(rename = "com.spotify.start_radio")]
  StartRadio,
  #[serde(rename = "com.spotify.superbird.dj.summon")]
  SummonDj,
  #[serde(rename = "com.spotify.superbird.instrumentation.request")]
  RequestLog(serde_json::Value),
  #[serde(rename = "com.spotify.superbird.instrumentation.interaction")]
  SendUbiInteraction(serde_json::Value),
  #[serde(rename = "com.spotify.superbird.instrumentation.impression")]
  SendUbiImpression(serde_json::Value),
  #[serde(rename = "com.spotify.superbird.instrumentation.log")]
  SendUbiBatch(serde_json::Value),
  #[serde(rename = "com.spotify.superbird.phone.answer")]
  PhoneAnswer {},
  #[serde(rename = "com.spotify.superbird.phone.decline")]
  PhoneDecline {},
  #[serde(rename = "com.spotify.superbird.phone.get_image")]
  PhoneCallImage { phone_number: String },
  #[serde(rename = "com.spotify.superbird.phone.send_message")]
  PhoneCallMessage { phone_number: String, message: String },
  #[serde(rename = "com.spotify.superbird.volume.volume_up")]
  IncreaseVolume {},
  #[serde(rename = "com.spotify.superbird.volume.volume_down")]
  DecreaseVolume {},
  #[serde(rename = "com.spotify.superbird.play_uri")]
  PlayUri {
    uri: String,
    feature_identifier: String,
    interaction_id: Option<String>,
    skip_to_uri: Option<String>,
    skip_to_uid: Option<String>,
  },
  #[serde(rename = "com.spotify.superbird.skip_next")]
  SkipNext {},
  #[serde(rename = "com.spotify.superbird.skip_prev")]
  SkipPrev { allow_seeking: bool },
  #[serde(rename = "com.spotify.superbird.seek_to")]
  SeekTo { position: usize },
  #[serde(rename = "com.spotify.superbird.resume")]
  Resume {},
  #[serde(rename = "com.spotify.superbird.pause")]
  Pause {},
  #[serde(rename = "com.spotify.superbird.set_shuffle")]
  SetShuffle { shuffle: bool },
  #[serde(rename = "com.spotify.superbird.set_repeat")]
  SetRepeat { repeat_mode: bool },
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StockInterAppSend {
  #[serde(rename = "msgId")]
  pub msg_id: Option<usize>,
  // msg_type: String,
  #[serde(flatten)]
  pub data: StockInterAppSendPayload,
}

#[serde_with::skip_serializing_none]
#[derive(derive_more::Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum StockInterAppSendPayload {
  #[serde(rename = "call_result")]
  Ack {},
  #[serde(rename = "call_result")]
  Permissions { can_use_superbird: bool },
  #[serde(rename = "com.spotify.session_state")]
  SessionState {
    connection_type: StockConnectionType,
    is_in_forced_offline_mode: bool,
    is_logged_in: bool,
    is_offline: bool,
  },
  #[serde(rename = "com.spotify.superbird.player_state")]
  IdlePlayerState {
    context_uri: String,
    is_paused: bool,
    is_paused_bool: bool,
    playback_options: StockPlaybackOptions,
    playback_position: usize,
    playback_restrictions: PlaybackRestrictions,
    playback_speed: f64, // TODO: this is a float. i don't care right now.
  },
  #[serde(rename = "com.spotify.superbird.player_state")]
  SimplePlayerState {
    currently_active_application: CurrentlyActiveApplication,
    context_uri: String,
    context_title: String,
    is_paused: bool,
    is_paused_bool: bool,
    playback_options: StockPlaybackOptions,
    playback_position: usize,
    playback_restrictions: PlaybackRestrictions,
    playback_speed: f64, // TODO: this is a float. i don't care right now.
    track: StockTrack,
  },
  #[serde(rename = "com.spotify.superbird.player_state")]
  SpotifyPlayerState {
    context_uri: String,
    context_title: String,
    is_paused: bool,
    is_paused_bool: bool,
    playback_options: StockPlaybackOptions,
    playback_position: usize,
    playback_restrictions: PlaybackRestrictions,
    playback_speed: f64, // TODO: this is a float. i don't care right now.
    track: StockTrack,
  },
  #[serde(rename = "com.spotify.play_queue")]
  PlayerQueue {
    next: Vec<StockQueueTrack>,
    current: StockQueueTrack,
    previous: Vec<StockQueueTrack>,
  },
  #[serde(rename = "com.spotify.get_children_of_item")]
  ItemChildren {
    limit: usize,
    offset: usize,
    total: usize,
    items: Vec<ChildItem>,
  },
  #[serde(rename = "call_result")]
  Image {
    height: usize,
    width: usize,
    #[debug(skip)]
    image_data: String,
  },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct StockPlaybackOptions {
  pub repeat: u32,
  pub shuffle: bool,
}

impl From<PlaybackOptions> for StockPlaybackOptions {
  fn from(options: PlaybackOptions) -> Self {
    Self {
      repeat: match options.repeat {
        RepeatMode::Off => 0,
        RepeatMode::One => 1,
        RepeatMode::All => 2,
      },
      shuffle: options.shuffle,
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChildItem {
  pub id: String,
  pub uri: String,
  pub image_id: String,
  pub title: String,
  pub subtitle: String,
  pub playable: bool,
  pub has_children: bool,
  pub available_offline: bool,
  pub metadata: ChildMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChildMeta {
  pub is_explicit_content: bool,
  pub is_19_plus_content: bool,
  pub duration_ms: usize,
}

impl From<BridgeToClientPlayerMsg> for StockInterAppSendPayload {
  fn from(data: BridgeToClientPlayerMsg) -> Self {
    match data {
      BridgeToClientPlayerMsg::PlayerIdle => Self::IdlePlayerState {
        context_uri: "".to_string(),
        is_paused: false,
        is_paused_bool: false,
        playback_position: 0,
        playback_options: StockPlaybackOptions::default(),
        playback_restrictions: PlaybackRestrictions::default(),
        playback_speed: 0.0,
      },
      BridgeToClientPlayerMsg::PlayerState(state) => Self::SpotifyPlayerState {
        context_uri: state.context_id,
        context_title: state.context_title,
        is_paused: state.is_paused,
        is_paused_bool: state.is_paused,
        playback_options: state.playback_options.into(),
        playback_position: state.playback_position,
        playback_restrictions: state.playback_restrictions,
        playback_speed: state.playback_speed,
        track: state.track.into(),
      },
      BridgeToClientPlayerMsg::Queue(queue) => Self::PlayerQueue {
        next: queue.next.into_iter().map(|a| a.into()).collect(),
        current: queue.current.into(),
        previous: queue.previous.into_iter().map(|a| a.into()).collect(),
      },
      BridgeToClientPlayerMsg::Image(image) => Self::Image {
        height: image.height,
        width: image.width,
        image_data: image.data,
      },
    }
  }
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockArtist {
  pub name: String,
  pub uri: String,
}

impl From<Artist> for StockArtist {
  fn from(artist: Artist) -> Self {
    Self {
      name: artist.name,
      uri: artist.id,
    }
  }
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockAlbum {
  pub name: String,
  pub uri: String,
}

impl From<Album> for StockAlbum {
  fn from(album: Album) -> Self {
    Self {
      name: album.name,
      uri: album.id,
    }
  }
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockTrack {
  pub name: String,
  pub album: StockAlbum,
  pub artist: StockArtist,
  pub artists: Vec<StockArtist>,
  pub duration_ms: usize,
  pub image_id: String,
  pub is_episode: bool,
  pub is_podcast: bool,
  pub saved: bool,
  pub uid: String,
  pub uri: String,
}

impl From<Track> for StockTrack {
  fn from(track: Track) -> Self {
    Self {
      name: track.name,
      album: track.album.into(),
      artist: track.artists.first().cloned().unwrap_or_default().into(),
      artists: track.artists.into_iter().map(|a| a.into()).collect(),
      duration_ms: track.duration_ms as usize,
      image_id: track.image_id,
      is_episode: false,
      is_podcast: false,
      saved: track.saved,
      uid: track.id.clone(),
      uri: track.id,
    }
  }
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockQueueTrack {
  pub uid: String,
  pub uri: String,
  pub name: String,
  pub artists: Vec<StockArtist>,
  pub image_uri: String,
  pub provider: String,
}

impl From<Track> for StockQueueTrack {
  fn from(track: Track) -> Self {
    Self {
      uid: track.id.clone(),
      uri: track.id,
      name: track.name,
      artists: track.artists.into_iter().map(|a| a.into()).collect(),
      image_uri: track.image_id,
      provider: "context".to_string(),
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StockConnectionType {
  None,
  Wlan,
  #[serde(rename = "4g")]
  FourG,
}

impl StockInterAppSend {
  pub fn new(msg_id: Option<usize>, data: StockInterAppSendPayload) -> Self {
    Self { msg_id, data }
  }

  pub fn make_ack(msg_id: Option<usize>) -> Self {
    Self {
      msg_id,
      data: StockInterAppSendPayload::Ack {},
    }
  }
}

impl From<BridgeToClientInteractionMsg> for StockInterAppSendPayload {
  fn from(payload: BridgeToClientInteractionMsg) -> Self {
    todo!()
  }
}

impl StockInterAppSend {
  pub fn from_interaction_send(send: BridgeToClientInteractionMsg, msg_id: Option<usize>) -> Self {
    Self {
      msg_id,
      data: send.into(),
    }
  }
}

impl RecvMsgData {
  /// this method WILL panic. don't be a dumbass.
  pub fn from_stock_inter_app_possible_recv(recv: PossibleRecvMsg) -> Self {
    let PossibleRecvMsg::StockInterApp { ref data, .. } = recv else {
      panic!("YOU CAN ONY CALL THIS WITH STOCK INTER APP RECV DATA");
    };

    // fully unsupported
    match data {
      StockInterAppRecv::GetChildrenOfItem { .. }
      | StockInterAppRecv::GetHome { .. }
      | StockInterAppRecv::GetPodcast { .. }
      | StockInterAppRecv::GetPresets { .. }
      | StockInterAppRecv::GetSaved { .. }
      | StockInterAppRecv::GetTips { .. }
      | StockInterAppRecv::GetTts { .. }
      | StockInterAppRecv::PlayPodcastTrailer { .. }
      | StockInterAppRecv::QueueUri { .. }
      | StockInterAppRecv::SetPodcastPlaybackSpeed { .. }
      | StockInterAppRecv::SetPreset { .. }
      | StockInterAppRecv::SetSaved { .. }
      | StockInterAppRecv::SummonDj
      | StockInterAppRecv::PlayUri { .. } => return RecvMsgData::Unsupported(recv),
      _ => {}
    }

    let PossibleRecvMsg::StockInterApp { data, .. } = recv else {
      panic!("YOU CAN ONY CALL THIS WITH STOCK INTER APP RECV DATA");
    };

    match data {
      StockInterAppRecv::GetImage { id } => RecvMsgData::LegacyStock(ClientLegacyStockCommand::GetImage { id }),
      StockInterAppRecv::GetThumbnailImage { id } => {
        RecvMsgData::LegacyStock(ClientLegacyStockCommand::GetThumbnailImage { id })
      }
      StockInterAppRecv::GetNextTracks {} => RecvMsgData::LegacyStock(ClientLegacyStockCommand::GetNextTracks),
      StockInterAppRecv::PhoneAnswer {} => RecvMsgData::Interaction(ClientToBridgeInteractionMsgCommand::PhoneAnswer),
      StockInterAppRecv::PhoneDecline {} => RecvMsgData::Interaction(ClientToBridgeInteractionMsgCommand::PhoneDecline),
      StockInterAppRecv::PhoneCallImage { phone_number } => RecvMsgData::Interaction(
        ClientToBridgeInteractionMsgCommand::PhoneCallImage(libbridgething::client::PhoneCallImage { phone_number }),
      ),
      StockInterAppRecv::PhoneCallMessage { phone_number, message } => {
        RecvMsgData::Interaction(ClientToBridgeInteractionMsgCommand::PhoneCallMessage(
          libbridgething::client::PhoneCallMessage { phone_number, message },
        ))
      }
      StockInterAppRecv::IncreaseVolume {} => {
        RecvMsgData::Interaction(ClientToBridgeInteractionMsgCommand::IncreaseVolume)
      }
      StockInterAppRecv::DecreaseVolume {} => {
        RecvMsgData::Interaction(ClientToBridgeInteractionMsgCommand::DecreaseVolume)
      }
      StockInterAppRecv::SkipToIndex { index } => RecvMsgData::Interaction(
        ClientToBridgeInteractionMsgCommand::SkipToIndex(libbridgething::client::SkipToIndex {
          index: u32::try_from(index).unwrap_or(u32::MAX),
        }),
      ),
      StockInterAppRecv::SkipNext {} => RecvMsgData::Interaction(ClientToBridgeInteractionMsgCommand::SkipNext),
      StockInterAppRecv::SkipPrev { allow_seeking: _ } => {
        RecvMsgData::Interaction(ClientToBridgeInteractionMsgCommand::SkipPrev)
      }
      StockInterAppRecv::SeekTo { position } => RecvMsgData::Interaction(ClientToBridgeInteractionMsgCommand::SeekTo(
        libbridgething::client::SeekTo {
          position_ms: u32::try_from(position).unwrap_or(u32::MAX),
        },
      )),
      StockInterAppRecv::Pause {} => RecvMsgData::Interaction(ClientToBridgeInteractionMsgCommand::Pause),
      StockInterAppRecv::Resume {} => RecvMsgData::Interaction(ClientToBridgeInteractionMsgCommand::Resume),
      StockInterAppRecv::SetShuffle { shuffle } => RecvMsgData::Interaction(
        ClientToBridgeInteractionMsgCommand::SetShuffle(libbridgething::client::SetShuffle { shuffle }),
      ),
      StockInterAppRecv::SetRepeat { repeat_mode } => {
        let mode = if repeat_mode {
          libbridgething::RepeatMode::All
        } else {
          libbridgething::RepeatMode::Off
        };
        RecvMsgData::Interaction(ClientToBridgeInteractionMsgCommand::SetRepeat(
          libbridgething::client::SetRepeat { repeat_mode: mode },
        ))
      }
      StockInterAppRecv::GetPermissions { .. } => {
        RecvMsgData::LegacyStock(ClientLegacyStockCommand::SpotifyGetPermissions)
      }

      _ => RecvMsgData::Hole,
    }
  }
}

#[cfg(test)]
mod test {
  use libbridgething::{
    ClientCommand, ClientToBridgeMsgData,
    client::{ClientLegacyStockCommand, ClientMsgMeta, ClientToBridgeInteractionMsg},
  };
  use uuid::Uuid;

  use super::StockInterAppRecv;
  use crate::handler::client::PossibleRecvMsg;

  #[test]
  fn ser_stock_recv() {
    let ser = serde_json::to_string(&StockInterAppRecv::PlayUri {
      uri: "test".to_string(),
      feature_identifier: "test".to_string(),
      interaction_id: None,
      skip_to_uri: None,
      skip_to_uid: None,
    })
    .expect("failed to serialize json");
    println!("{:?}", &ser);

    assert_eq!(
      ser,
      r#"{"method":"com.spotify.superbird.play_uri","args":{"uri":"test","feature_identifier":"test"}}"#
    );
  }

  #[test]
  fn de_stock_recv_play_uri() {
    let json = r#"{"msgId":69,"method":"com.spotify.superbird.play_uri","args":{"uri":"test","feature_identifier":"test"},"userAction": true}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::StockInterApp {
        msg_id: 69,
        data: StockInterAppRecv::PlayUri {
          uri: "test".to_string(),
          feature_identifier: "test".to_string(),
          interaction_id: None,
          skip_to_uri: None,
          skip_to_uid: None,
        },
        user_action: true,
      }
    );
  }

  #[test]
  fn de_stock_recv_get_image() {
    let json = r#"{"msgId":1,"method":"com.spotify.get_image","args":{"id":"image_id"},"userAction": false}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::StockInterApp {
        msg_id: 1,
        data: StockInterAppRecv::GetImage {
          id: "image_id".to_string(),
        },
        user_action: false,
      }
    );
  }

  #[test]
  fn de_stock_recv_phone_call_message() {
    let json = r#"{"msgId":2,"method":"com.spotify.superbird.phone.send_message","args":{"phone_number":"1234567890","message":"Hello"},"userAction": false}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::StockInterApp {
        msg_id: 2,
        data: StockInterAppRecv::PhoneCallMessage {
          phone_number: "1234567890".to_string(),
          message: "Hello".to_string(),
        },
        user_action: false,
      }
    );
  }

  #[test]
  fn de_stock_recv_increase_volume() {
    let json = r#"{"msgId":3,"method":"com.spotify.superbird.volume.volume_up","args":{},"userAction": false}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::StockInterApp {
        msg_id: 3,
        data: StockInterAppRecv::IncreaseVolume {},
        user_action: false,
      }
    );
  }

  #[test]
  fn de_stock_recv_skip_to_index() {
    let json = r#"{"msgId":4,"method":"com.spotify.skip_to_index_in_queue","args":{"index":5},"userAction": false}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::StockInterApp {
        msg_id: 4,
        data: StockInterAppRecv::SkipToIndex { index: 5 },
        user_action: false,
      }
    );
  }

  #[test]
  fn de_stock_recv_set_repeat() {
    let json =
      r#"{"msgId":5,"method":"com.spotify.superbird.set_repeat","args":{"repeat_mode":true},"userAction": false}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::StockInterApp {
        msg_id: 5,
        data: StockInterAppRecv::SetRepeat { repeat_mode: true },
        user_action: false,
      }
    );
  }

  #[test]
  fn de_stock_recv_permissions() {
    let json = r#"{"msgId":2,"method":"com.spotify.superbird.permissions","args":{},"userAction":false}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);
  }

  #[test]
  fn de_stock_recv_instrumentation() {
    let json = r#"{"msgId":3,"method":"com.spotify.superbird.instrumentation.log","args":{"interactions":[{"action_name":"ui_navigate_back","action_version":1,"annotator_configuration_version":"","annotator_version":"","app":"music","element_path_ids":["","","",""],"element_path_names":["car-settings-carthingos","phone_connection_row","phone_connection_view","hardware_back_button"],"element_path_pos":["","","",""],"element_path_reasons":["","","",""],"element_path_uris":["","","",""],"generator_version":"10.0.2","interaction_id":"a27d88eb-1b5f-4f57-b477-51b3f031b9f1","interaction_type":"key_stroke","parent_modes":[],"parent_path_ids":[],"parent_path_names":[],"parent_path_pos":[],"parent_path_reasons":[],"parent_path_uris":[],"parent_specification_versions":[],"specification_version":"9.0.0","specification_mode":"default","page_instance_id":null,"playback_id":null,"play_context_uri":null},{"action_name":"ui_navigate_back","action_version":1,"annotator_configuration_version":"","annotator_version":"","app":"music","element_path_ids":["",""],"element_path_names":["car-settings-carthingos","hardware_back_button"],"element_path_pos":["",""],"element_path_reasons":["",""],"element_path_uris":["",""],"generator_version":"10.0.2","interaction_id":"4a212c49-76a7-4f34-afad-9c03052c4e4b","interaction_type":"key_stroke","parent_modes":[],"parent_path_ids":[],"parent_path_names":[],"parent_path_pos":[],"parent_path_reasons":[],"parent_path_uris":[],"parent_specification_versions":[],"specification_version":"9.0.0","specification_mode":"default","page_instance_id":null,"playback_id":null,"play_context_uri":null}],"impressions":[{"annotator_configuration_version":"","annotator_version":"","app":"music","element_path_ids":["","","","",""],"element_path_names":["car-settings-carthingos","phone_connection_row","phone_connection_view","existing_phone_row","select_phone_progress_dialog"],"element_path_pos":["","","","",""],"element_path_reasons":["","","","",""],"element_path_uris":["","","","",""],"generator_version":"10.0.2","impression_id":"b858b7aa-bbb9-4816-89ee-25ca1602eb91","parent_modes":[],"parent_path_ids":[],"parent_path_names":[],"parent_path_pos":[],"parent_path_reasons":[],"parent_path_uris":[],"parent_specification_versions":[],"specification_version":"9.0.0","specification_mode":"default","page_instance_id":null,"playback_id":null,"play_context_uri":null},{"annotator_configuration_version":"","annotator_version":"","app":"music","element_path_ids":["","","","",""],"element_path_names":["car-settings-carthingos","phone_connection_row","phone_connection_view","existing_phone_row","select_phone_success_dialog"],"element_path_pos":["","","","",""],"element_path_reasons":["","","","",""],"element_path_uris":["","","","",""],"generator_version":"10.0.2","impression_id":"7eff4cb6-384c-4f43-8ffd-f9e16366ab22","parent_modes":[],"parent_path_ids":[],"parent_path_names":[],"parent_path_pos":[],"parent_path_reasons":[],"parent_path_uris":[],"parent_specification_versions":[],"specification_version":"9.0.0","specification_mode":"default","page_instance_id":null,"playback_id":null,"play_context_uri":null},{"annotator_configuration_version":"","annotator_version":"","app":"music","element_path_ids":[""],"element_path_names":["car-settings-carthingos"],"element_path_pos":[""],"element_path_reasons":[""],"element_path_uris":[""],"generator_version":"10.0.2","impression_id":"50ccf04d-e308-4cf6-a1e0-fd57d7913d63","parent_modes":[],"parent_path_ids":[],"parent_path_names":[],"parent_path_pos":[],"parent_path_reasons":[],"parent_path_uris":[],"parent_specification_versions":[],"specification_version":"9.0.0","specification_mode":"default","page_instance_id":null,"playback_id":null,"play_context_uri":null}],"interaction_timestamps":[{"timestamp":1733803725274,"interaction_id":"a27d88eb-1b5f-4f57-b477-51b3f031b9f1"},{"timestamp":1733803726813,"interaction_id":"4a212c49-76a7-4f34-afad-9c03052c4e4b"}],"impression_timestamps":[{"timestamp":1733803720258,"impression_id":"b858b7aa-bbb9-4816-89ee-25ca1602eb91"},{"timestamp":1733803720263,"impression_id":"7eff4cb6-384c-4f43-8ffd-f9e16366ab22"},{"timestamp":1733803725285,"impression_id":"50ccf04d-e308-4cf6-a1e0-fd57d7913d63"}]},"userAction":false}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);
  }

  #[test]
  fn ser_recv_get_image() {
    let ser = serde_json::to_string(&PossibleRecvMsg::Modern(ClientCommand {
      id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
      meta: ClientMsgMeta::Request,
      data: ClientToBridgeMsgData::LegacyStock(ClientLegacyStockCommand::GetImage {
        id: "image_id".to_string(),
      }),
    }))
    .expect("failed to serialize json");
    println!("{:?}", &ser);

    assert_eq!(
      ser,
      r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","meta":{"kind":"request"},"type":"legacyStock","action":"getImage","args":{"id":"image_id"}}"#
    );
  }

  #[test]
  fn de_recv_phone_call_image() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","meta":{"kind":"command"},"type":"interaction","action":"phoneCallImage","args":{"phoneNumber":"1234567890"}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ClientCommand {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        meta: ClientMsgMeta::Command,
        data: ClientToBridgeMsgData::Interaction(ClientToBridgeInteractionMsg::PhoneCallImage {
          phone_number: "1234567890".to_string()
        })
      })
    );
  }

  #[test]
  fn de_recv_phone_call_message() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","meta":{"kind":"command"},"type":"interaction","action":"phoneCallMessage","args":{"phoneNumber":"1234567890","message":"Hello"}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ClientCommand {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        meta: ClientMsgMeta::Command,
        data: ClientToBridgeMsgData::Interaction(ClientToBridgeInteractionMsg::PhoneCallMessage {
          phone_number: "1234567890".to_string(),
          message: "Hello".to_string()
        })
      })
    );
  }

  #[test]
  fn de_recv_skip_to_index() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","meta":{"kind":"command"},"type":"interaction","action":"skipToIndex","args":{"index":5}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ClientCommand {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        meta: ClientMsgMeta::Command,
        data: ClientToBridgeMsgData::Interaction(ClientToBridgeInteractionMsg::SkipToIndex { index: 5 })
      })
    );
  }

  #[test]
  fn de_recv_skip_prev() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","meta":{"kind":"command"},"type":"interaction","action":"skipPrev"}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ClientCommand {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        meta: ClientMsgMeta::Command,
        data: ClientToBridgeMsgData::Interaction(ClientToBridgeInteractionMsg::SkipPrev)
      })
    );
  }

  #[test]
  fn de_recv_seek_to() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","meta":{"kind":"command"},"type":"interaction","action":"seekTo","args":{"positionMs":120}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ClientCommand {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        meta: ClientMsgMeta::Command,
        data: ClientToBridgeMsgData::Interaction(ClientToBridgeInteractionMsg::SeekTo { position_ms: 120 })
      })
    );
  }

  #[test]
  fn de_recv_set_shuffle() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","meta":{"kind":"command"},"type":"interaction","action":"setShuffle","args":{"shuffle":true}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ClientCommand {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        meta: ClientMsgMeta::Command,
        data: ClientToBridgeMsgData::Interaction(ClientToBridgeInteractionMsg::SetShuffle { shuffle: true })
      })
    );
  }

  #[test]
  fn de_recv_set_repeat() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","meta":{"kind":"command"},"type":"interaction","action":"setRepeat","args":{"repeatMode":"all"}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ClientCommand {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        meta: ClientMsgMeta::Command,
        data: ClientToBridgeMsgData::Interaction(ClientToBridgeInteractionMsg::SetRepeat {
          repeat_mode: libbridgething::RepeatMode::All
        })
      })
    );
  }
}
