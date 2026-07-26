use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::NluSlots;

/// The display-shaped intents
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub enum VoiceDisplayIntent {
  Search,
  ShowView,
  MoreLikeThis,
}

/// A resolved display intent for the active webapp to render
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct VoiceIntent {
  pub intent: VoiceDisplayIntent,
  #[serde(default)]
  pub slots: NluSlots,
  pub transcript: String,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Current mic state.
pub struct VoiceState {
  pub muted: bool,
  /// True while a capture session is actively recording.
  pub capturing: bool,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Response to `voice.stateGet`.
pub struct VoiceStateReply {
  pub state: VoiceState,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
/// Daemon -> webapp voice/NLU surface: mic state-change events, resolved
/// display intents, and the reply to `voice.stateGet`.
pub enum BridgeToClientVoiceMsg {
  #[bridge_event]
  StateChanged(VoiceState),
  #[bridge_event]
  Intent(VoiceIntent),
  #[bridge_response]
  StateReply(VoiceStateReply),
}
