use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

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
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
/// Daemon -> webapp voice/NLU surface: mic state-change events and the
/// reply to `voice.stateGet`.
pub enum BridgeToClientVoiceMsg {
  #[bridge_event]
  StateChanged(VoiceState),
  #[bridge_response]
  StateReply(VoiceStateReply),
}
