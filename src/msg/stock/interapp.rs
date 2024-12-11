use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::msg::{InteractionRecv, RecvMsgData};

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
  #[serde(rename = "com.spotify.get_player_state")]
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

impl From<(usize, StockInterAppRecv)> for RecvMsgData {
  fn from((msg_id, data): (usize, StockInterAppRecv)) -> Self {
    match data {
      StockInterAppRecv::GetImage { id } => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::GetImage { id },
      },
      StockInterAppRecv::GetNextTracks {} => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::GetNextTracks,
      },
      StockInterAppRecv::PhoneAnswer {} => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::PhoneAnswer,
      },
      StockInterAppRecv::PhoneDecline {} => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::PhoneDecline,
      },
      StockInterAppRecv::PhoneCallImage { phone_number } => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::PhoneCallImage { phone_number },
      },
      StockInterAppRecv::PhoneCallMessage { phone_number, message } => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::PhoneCallMessage { phone_number, message },
      },
      StockInterAppRecv::IncreaseVolume {} => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::IncreaseVolume,
      },
      StockInterAppRecv::DecreaseVolume {} => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::DecreaseVolume,
      },
      StockInterAppRecv::SkipToIndex { index } => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::SkipToIndex { index },
      },
      StockInterAppRecv::SkipNext {} => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::SkipNext,
      },
      StockInterAppRecv::SkipPrev { allow_seeking } => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::SkipPrev { allow_seeking },
      },
      StockInterAppRecv::SeekTo { position } => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::SeekTo { position },
      },
      StockInterAppRecv::Pause {} => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::Pause,
      },
      StockInterAppRecv::Resume {} => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::Resume,
      },
      StockInterAppRecv::SetShuffle { shuffle } => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::SetShuffle { shuffle },
      },
      StockInterAppRecv::SetRepeat { repeat_mode } => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::SetRepeat { repeat_mode },
      },
      StockInterAppRecv::GetChildrenOfItem {
        parent_id,
        limit,
        offset,
      } => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::SpotifyGetChildren {
          parent_id,
          limit,
          offset,
        },
      },
      StockInterAppRecv::GetHome { limit, limit_overrides } => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::SpotifyGetHome { limit, limit_overrides },
      },
      StockInterAppRecv::GetPermissions {} => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::SpotifyGetPermissions,
      },
      StockInterAppRecv::GetPodcast { uri, limit, offset } => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::SpotifyGetPodcast { uri, limit, offset },
      },
      StockInterAppRecv::GetPresets {} => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::SpotifyGetPresets,
      },
      StockInterAppRecv::GetSaved { id } => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::SpotifyGetSaved { id },
      },
      StockInterAppRecv::GetThumbnailImage { id } => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::GetThumbnailImage { id },
      },
      StockInterAppRecv::GetTips {} => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::SpotifyGetTips,
      },
      StockInterAppRecv::GetTts { file } => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::SpotifyGetTts { file },
      },
      StockInterAppRecv::PlayPodcastTrailer { uri } => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::SpotifyPlayPodcastTrailer { uri },
      },
      StockInterAppRecv::QueueUri { uri } => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::SpotifyQueueUri { uri },
      },
      StockInterAppRecv::SetPodcastPlaybackSpeed { playback_speed } => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::SpotifySetPodcastPlaybackSpeed { playback_speed },
      },
      StockInterAppRecv::SetPreset { presets } => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::SpotifySetPreset { presets },
      },
      StockInterAppRecv::SetSaved { id, uri, saved } => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::SpotifySetSaved { id, uri, saved },
      },
      StockInterAppRecv::SummonDj => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::SpotifySummonDj,
      },
      StockInterAppRecv::PlayUri {
        uri,
        feature_identifier,
        interaction_id,
        skip_to_uri,
        skip_to_uid,
      } => RecvMsgData::Interaction {
        stock_msg_id: Some(msg_id),
        msg: InteractionRecv::SpotifyPlayUri {
          uri,
          feature_identifier,
          interaction_id,
          skip_to_uri,
          skip_to_uid,
        },
      },
      _ => RecvMsgData::Hole,
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct StockSetPreset {
  pub version: usize, // 1
  pub context_uri: String,
  pub slot_index: usize, // 1-4
  pub source: String,    // 'tactile' | 'voice'
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct StockPreset {
  pub context_uri: String,
  pub image_url: Option<String>,
  pub slot_index: usize, // 1-4
  pub name: Option<String>,
  pub description: Option<String>,
}

#[cfg(test)]
mod test {
  use super::StockInterAppRecv;
  use crate::msg::PossibleRecvMsg;

  #[test]
  fn ser_stock_recv() {
    let ser = serde_json::to_string(&StockInterAppRecv::PlayUri {
      uri: "test".to_owned(),
      feature_identifier: "test".to_owned(),
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
          uri: "test".to_owned(),
          feature_identifier: "test".to_owned(),
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
          id: "image_id".to_owned(),
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
          phone_number: "1234567890".to_owned(),
          message: "Hello".to_owned(),
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
}
