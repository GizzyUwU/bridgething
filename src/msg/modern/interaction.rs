use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::msg::StockSetPreset;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", content = "args", rename_all = "camelCase")]
pub enum InteractionRecv {
  GetImage {
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
  SpotifyGetHome {
    limit: usize,
    limit_overrides: HashMap<String, usize>,
  },
  SpotifyGetPermissions,
  SpotifyGetPodcast {
    uri: String,
    limit: Option<usize>,
    offset: Option<usize>,
  },
  SpotifyGetPresets,
  SpotifyGetSaved {
    id: String,
  },
  GetThumbnailImage {
    id: String,
  },
  SpotifyGetTips,
  SpotifyGetTts {
    file: String,
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
  SpotifySetPreset {
    presets: Vec<StockSetPreset>,
  },
  SpotifySetSaved {
    id: Option<String>, // id is same as uri
    uri: Option<String>,
    saved: bool,
  },

  SpotifySummonDj,
  SpotifyPlayUri {
    uri: String,
    feature_identifier: String,
    interaction_id: Option<String>,
    skip_to_uri: Option<String>,
    skip_to_uid: Option<String>,
  },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", content = "data", rename_all = "camelCase")]
pub enum InteractionSend {}

#[cfg(test)]
mod test {
  use std::collections::HashMap;

  use uuid::Uuid;

  use crate::msg::{InteractionRecv, ModernRecvMsg, ModernRecvMsgType, PossibleRecvMsg, StockSetPreset};

  #[test]
  fn ser_recv_get_image() {
    let ser = serde_json::to_string(&PossibleRecvMsg::Modern(ModernRecvMsg {
      id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
      data: ModernRecvMsgType::Interaction {
        msg: InteractionRecv::GetImage {
          id: "image_id".to_string(),
        },
        stock_msg_id: None,
      },
    }))
    .expect("failed to serialize json");
    println!("{:?}", &ser);

    assert_eq!(
      ser,
      r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","type":"interaction","action":"getImage","args":{"id":"image_id"}}"#
    );
  }

  #[test]
  fn de_recv_get_image() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","type":"interaction","action":"getImage","args":{"id":"image_id"}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ModernRecvMsg {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        data: ModernRecvMsgType::Interaction {
          msg: InteractionRecv::GetImage {
            id: "image_id".to_string()
          },
          stock_msg_id: None
        }
      })
    );
  }

  #[test]
  fn de_recv_get_next_tracks() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","type":"interaction","action":"getNextTracks"}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ModernRecvMsg {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        data: ModernRecvMsgType::Interaction {
          msg: InteractionRecv::GetNextTracks,
          stock_msg_id: None
        }
      })
    );
  }

  #[test]
  fn de_recv_phone_call_image() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","type":"interaction","action":"phoneCallImage","args":{"phone_number":"1234567890"}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ModernRecvMsg {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        data: ModernRecvMsgType::Interaction {
          msg: InteractionRecv::PhoneCallImage {
            phone_number: "1234567890".to_string()
          },
          stock_msg_id: None
        }
      })
    );
  }

  #[test]
  fn de_recv_phone_call_message() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","type":"interaction","action":"phoneCallMessage","args":{"phone_number":"1234567890","message":"Hello"}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ModernRecvMsg {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        data: ModernRecvMsgType::Interaction {
          msg: InteractionRecv::PhoneCallMessage {
            phone_number: "1234567890".to_string(),
            message: "Hello".to_string()
          },
          stock_msg_id: None
        }
      })
    );
  }

  #[test]
  fn de_recv_skip_to_index() {
    let json =
      r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","type":"interaction","action":"skipToIndex","args":{"index":5}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ModernRecvMsg {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        data: ModernRecvMsgType::Interaction {
          msg: InteractionRecv::SkipToIndex { index: 5 },
          stock_msg_id: None
        }
      })
    );
  }

  #[test]
  fn de_recv_skip_prev() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","type":"interaction","action":"skipPrev","args":{"allow_seeking":true}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ModernRecvMsg {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        data: ModernRecvMsgType::Interaction {
          msg: InteractionRecv::SkipPrev { allow_seeking: true },
          stock_msg_id: None
        }
      })
    );
  }

  #[test]
  fn de_recv_seek_to() {
    let json =
      r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","type":"interaction","action":"seekTo","args":{"position":120}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ModernRecvMsg {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        data: ModernRecvMsgType::Interaction {
          msg: InteractionRecv::SeekTo { position: 120 },
          stock_msg_id: None
        }
      })
    );
  }

  #[test]
  fn de_recv_set_shuffle() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","type":"interaction","action":"setShuffle","args":{"shuffle":true}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ModernRecvMsg {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        data: ModernRecvMsgType::Interaction {
          msg: InteractionRecv::SetShuffle { shuffle: true },
          stock_msg_id: None
        }
      })
    );
  }

  #[test]
  fn de_recv_set_repeat() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","type":"interaction","action":"setRepeat","args":{"repeat_mode":true}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ModernRecvMsg {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        data: ModernRecvMsgType::Interaction {
          msg: InteractionRecv::SetRepeat { repeat_mode: true },
          stock_msg_id: None
        }
      })
    );
  }

  #[test]
  fn de_recv_spotify_get_children() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","type":"interaction","action":"spotifyGetChildren","args":{"parent_id":"parent_id","limit":10,"offset":5}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ModernRecvMsg {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        data: ModernRecvMsgType::Interaction {
          msg: InteractionRecv::SpotifyGetChildren {
            parent_id: "parent_id".to_string(),
            limit: 10,
            offset: Some(5)
          },
          stock_msg_id: None
        }
      })
    );
  }

  #[test]
  fn de_recv_spotify_get_home() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","type":"interaction","action":"spotifyGetHome","args":{"limit":10,"limit_overrides":{"key1":5,"key2":10}}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    let mut limit_overrides = HashMap::new();
    limit_overrides.insert("key1".to_string(), 5);
    limit_overrides.insert("key2".to_string(), 10);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ModernRecvMsg {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        data: ModernRecvMsgType::Interaction {
          msg: InteractionRecv::SpotifyGetHome {
            limit: 10,
            limit_overrides
          },
          stock_msg_id: None
        }
      })
    );
  }

  #[test]
  fn de_recv_spotify_get_podcast() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","type":"interaction","action":"spotifyGetPodcast","args":{"uri":"podcast_uri","limit":10,"offset":5}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ModernRecvMsg {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        data: ModernRecvMsgType::Interaction {
          msg: InteractionRecv::SpotifyGetPodcast {
            uri: "podcast_uri".to_string(),
            limit: Some(10),
            offset: Some(5)
          },
          stock_msg_id: None
        }
      })
    );
  }

  #[test]
  fn de_recv_spotify_get_saved() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","type":"interaction","action":"spotifyGetSaved","args":{"id":"saved_id"}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ModernRecvMsg {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        data: ModernRecvMsgType::Interaction {
          msg: InteractionRecv::SpotifyGetSaved {
            id: "saved_id".to_string()
          },
          stock_msg_id: None
        }
      })
    );
  }

  #[test]
  fn de_recv_get_thumbnail_image() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","type":"interaction","action":"getThumbnailImage","args":{"id":"thumbnail_id"}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ModernRecvMsg {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        data: ModernRecvMsgType::Interaction {
          msg: InteractionRecv::GetThumbnailImage {
            id: "thumbnail_id".to_string()
          },
          stock_msg_id: None
        }
      })
    );
  }

  #[test]
  fn de_recv_spotify_get_tts() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","type":"interaction","action":"spotifyGetTts","args":{"file":"tts_file"}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ModernRecvMsg {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        data: ModernRecvMsgType::Interaction {
          msg: InteractionRecv::SpotifyGetTts {
            file: "tts_file".to_string()
          },
          stock_msg_id: None
        }
      })
    );
  }

  #[test]
  fn de_recv_spotify_play_podcast_trailer() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","type":"interaction","action":"spotifyPlayPodcastTrailer","args":{"uri":"trailer_uri"}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ModernRecvMsg {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        data: ModernRecvMsgType::Interaction {
          msg: InteractionRecv::SpotifyPlayPodcastTrailer {
            uri: "trailer_uri".to_string()
          },
          stock_msg_id: None
        }
      })
    );
  }

  #[test]
  fn de_recv_spotify_queue_uri() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","type":"interaction","action":"spotifyQueueUri","args":{"uri":"queue_uri"}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ModernRecvMsg {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        data: ModernRecvMsgType::Interaction {
          msg: InteractionRecv::SpotifyQueueUri {
            uri: "queue_uri".to_string()
          },
          stock_msg_id: None
        }
      })
    );
  }

  #[test]
  fn de_recv_spotify_set_podcast_playback_speed() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","type":"interaction","action":"spotifySetPodcastPlaybackSpeed","args":{"playback_speed":2}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ModernRecvMsg {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        data: ModernRecvMsgType::Interaction {
          msg: InteractionRecv::SpotifySetPodcastPlaybackSpeed { playback_speed: 2 },
          stock_msg_id: None
        }
      })
    );
  }

  #[test]
  fn de_recv_spotify_set_preset() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","type":"interaction","action":"spotifySetPreset","args":{"presets":[{"version":1,"context_uri":"context1","slot_index":0,"source":"source1"},{"version":2,"context_uri":"context2","slot_index":1,"source":"source2"}]}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    let presets = vec![
      StockSetPreset {
        version: 1,
        context_uri: "context1".to_string(),
        slot_index: 0,
        source: "source1".to_string(),
      },
      StockSetPreset {
        version: 2,
        context_uri: "context2".to_string(),
        slot_index: 1,
        source: "source2".to_string(),
      },
    ];

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ModernRecvMsg {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        data: ModernRecvMsgType::Interaction {
          msg: InteractionRecv::SpotifySetPreset { presets },
          stock_msg_id: None
        }
      })
    );
  }

  #[test]
  fn de_recv_spotify_set_saved() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","type":"interaction","action":"spotifySetSaved","args":{"id":"saved_id","uri":"saved_uri","saved":true}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ModernRecvMsg {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        data: ModernRecvMsgType::Interaction {
          msg: InteractionRecv::SpotifySetSaved {
            id: Some("saved_id".to_string()),
            uri: Some("saved_uri".to_string()),
            saved: true
          },
          stock_msg_id: None
        }
      })
    );
  }

  #[test]
  fn de_recv_spotify_play_uri() {
    let json = r#"{"id":"0193ace5-1876-7b2c-8d7b-f63a20d6f316","type":"interaction","action":"spotifyPlayUri","args":{"uri":"play_uri","feature_identifier":"feature_id","interaction_id":"interaction_id","skip_to_uri":"skip_uri","skip_to_uid":"skip_uid"}}"#;
    let de: PossibleRecvMsg = serde_json::from_str(json).expect("failed to deserialize json");
    println!("{:?}", de);

    assert_eq!(
      de,
      PossibleRecvMsg::Modern(ModernRecvMsg {
        id: Uuid::parse_str("0193ace5-1876-7b2c-8d7b-f63a20d6f316").unwrap(),
        data: ModernRecvMsgType::Interaction {
          msg: InteractionRecv::SpotifyPlayUri {
            uri: "play_uri".to_string(),
            feature_identifier: "feature_id".to_string(),
            interaction_id: Some("interaction_id".to_string()),
            skip_to_uri: Some("skip_uri".to_string()),
            skip_to_uid: Some("skip_uid".to_string())
          },
          stock_msg_id: None
        }
      })
    );
  }
}
