use libbridgething::{
  NluTargetType, NluView,
  client::{ClientToBridgeVoiceMsg, MicMute, MicUnmute, VoiceDisplayIntent, VoiceIntent},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum StockVoiceRecv {
  Cancel,
  PushToTalk,
  MuteMic { attributes: MuteStatusAttributes },
  UnmuteMic { attributes: MuteStatusAttributes },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MuteStatusAttributes {
  preserve: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StockVoiceSend {
  #[serde(rename = "voice_wakeword")]
  WakeWord {
    reason: StockWakeWord,
  },
  #[serde(rename = "voice_local_command")]
  LocalCommand {
    command: serde_json::Value,
  },
  #[serde(rename = "voice_intermediate_result")]
  IntermediateResult {
    payload: serde_json::Value,
  },
  #[serde(rename = "voice_intent")]
  Intent {
    payload: serde_json::Value,
  },
  #[serde(rename = "voice_mute")]
  Mute {
    payload: bool,
  },
  #[serde(rename = "voice_microphone_level")]
  MicrophoneLevel {
    level: String,
  },
  #[serde(rename = "voice_timeout")]
  Timeout,
  Error {
    payload: StockVoiceErrorPayload,
  },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StockWakeWord {
  None,
  HeySpotify,
  OkSpotify,
  PushToTalk,
  UserRequest,
  Enrolled,
  #[serde(rename = "UNKOWN")] // yes this is intentional - spotify misspelled it.
  Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockVoiceErrorPayload {
  cause: String,
  domain: String,
}

impl From<StockVoiceRecv> for ClientToBridgeVoiceMsg {
  fn from(data: StockVoiceRecv) -> Self {
    match data {
      StockVoiceRecv::Cancel => ClientToBridgeVoiceMsg::Cancel,
      StockVoiceRecv::PushToTalk => ClientToBridgeVoiceMsg::PushToTalk,
      StockVoiceRecv::MuteMic { attributes } => ClientToBridgeVoiceMsg::MuteMic(MicMute {
        preserve: attributes.preserve,
      }),
      StockVoiceRecv::UnmuteMic { attributes } => ClientToBridgeVoiceMsg::UnmuteMic(MicUnmute {
        preserve: attributes.preserve,
      }),
    }
  }
}

pub fn voice_intent_to_stock(intent: &VoiceIntent) -> StockVoiceSend {
  let (slimo_intent, action) = match intent.intent {
    VoiceDisplayIntent::Search => ("SEARCH", search_action(&intent.slots)),
    VoiceDisplayIntent::MoreLikeThis => ("MORE_LIKE_THIS", "UNKNOWN"),
    VoiceDisplayIntent::ShowView => {
      let name = view_name(intent.slots.view);
      (name, name)
    }
  };

  let mut custom = serde_json::json!({
    "intent": slimo_intent,
    "action": action,
    "query": intent.transcript,
    "connect_action_taken": false,
  });

  if let Some(target_type) = intent.slots.target_type
    && let Ok(serde_json::Value::String(name)) = serde_json::to_value(target_type)
  {
    custom["slots"] = serde_json::json!({ "requestedEntityType": [name] });
  }
  if let Ok(bridge_slots) = serde_json::to_value(&intent.slots) {
    custom["bridge_slots"] = bridge_slots;
  }

  StockVoiceSend::Intent {
    payload: serde_json::json!({ "custom": custom }),
  }
}

fn search_action(slots: &libbridgething::NluSlots) -> &'static str {
  match slots.target_type {
    Some(NluTargetType::Playlist) | Some(NluTargetType::Track) => "SHOW_TRACKS",
    Some(NluTargetType::Podcast) | Some(NluTargetType::Episode) => "SHOW_PODCAST",
    _ => "UNKNOWN",
  }
}

fn view_name(view: Option<NluView>) -> &'static str {
  match view {
    Some(NluView::NowPlaying) => "SHOW_NOW_PLAYING",
    Some(NluView::Artist) | Some(NluView::Album) => "SHOW_THIS_ARTIST",
    Some(NluView::Playlist) => "SHOW_TRACKS",
    Some(NluView::Playlists) | Some(NluView::Library) => "SHOW_MY_LIBRARY",
    Some(NluView::Presets) => "SHOW_MY_PRESETS",
    Some(NluView::Songs) => "SHOW_MY_SONGS",
    Some(NluView::SavedEpisodes) => "SHOW_MY_SAVED_EPISODES",
    Some(NluView::NewEpisodes) => "SHOW_MY_NEW_EPISODES",
    Some(NluView::Queue) => "SHOW_THE_QUEUE",
    None => "UNKNOWN",
  }
}

#[cfg(test)]
mod tests {
  use libbridgething::NluSlots;

  use super::*;

  fn intent(intent: VoiceDisplayIntent, slots: NluSlots) -> VoiceIntent {
    VoiceIntent {
      intent,
      slots,
      transcript: "show me my songs".into(),
    }
  }

  fn custom(sent: &StockVoiceSend) -> serde_json::Value {
    match sent {
      StockVoiceSend::Intent { payload } => payload["custom"].clone(),
      other => panic!("expected a voice_intent, got {other:?}"),
    }
  }

  #[test]
  fn show_view_expands_to_the_slimo_action_stock_routes_on() {
    let slots = NluSlots {
      view: Some(NluView::Songs),
      ..Default::default()
    };
    let c = custom(&voice_intent_to_stock(&intent(VoiceDisplayIntent::ShowView, slots)));
    assert_eq!(c["intent"], "SHOW_MY_SONGS");
    assert_eq!(c["action"], "SHOW_MY_SONGS");
  }

  #[test]
  fn every_view_maps_to_a_show_prefixed_action() {
    let views = [
      NluView::NowPlaying,
      NluView::Artist,
      NluView::Album,
      NluView::Playlist,
      NluView::Playlists,
      NluView::Library,
      NluView::Presets,
      NluView::Songs,
      NluView::SavedEpisodes,
      NluView::NewEpisodes,
      NluView::Queue,
    ];
    for view in views {
      let name = view_name(Some(view));
      assert!(
        name.starts_with("SHOW_"),
        "{view:?} produced {name}, which stock cannot route"
      );
    }
  }

  #[test]
  fn search_naming_a_playlist_renders_an_entity_page() {
    let slots = NluSlots {
      target: Some("discover weekly".into()),
      target_type: Some(NluTargetType::Playlist),
      ..Default::default()
    };
    let c = custom(&voice_intent_to_stock(&intent(VoiceDisplayIntent::Search, slots)));
    assert_eq!(c["intent"], "SEARCH");
    assert_eq!(c["action"], "SHOW_TRACKS");
  }

  #[test]
  fn search_with_only_an_untyped_target_stays_a_result_list() {
    let slots = NluSlots {
      target: Some("shoegaze".into()),
      ..Default::default()
    };
    let c = custom(&voice_intent_to_stock(&intent(VoiceDisplayIntent::Search, slots)));
    assert_eq!(c["action"], "UNKNOWN", "an untyped target has no entity page to open");
  }

  #[test]
  fn target_type_is_wrapped_in_an_array_for_stock() {
    let slots = NluSlots {
      target: Some("blue rev".into()),
      target_type: Some(NluTargetType::Album),
      ..Default::default()
    };
    let c = custom(&voice_intent_to_stock(&intent(VoiceDisplayIntent::Search, slots)));
    assert_eq!(c["slots"]["requestedEntityType"][0], "album");
    assert_eq!(c["bridge_slots"]["targetType"], "album");
  }

  #[test]
  fn display_intents_never_claim_a_playback_action() {
    let c = custom(&voice_intent_to_stock(&intent(
      VoiceDisplayIntent::MoreLikeThis,
      NluSlots::default(),
    )));
    assert_eq!(c["connect_action_taken"], false);
  }
}
