use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::shared::RepeatMode;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PhoneCallImage {
  pub phone_number: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct PhoneCallMessage {
  pub phone_number: String,
  pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct SkipToIndex {
  pub index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct SeekTo {
  pub position_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct SetShuffle {
  pub shuffle: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct SetRepeat {
  pub repeat_mode: RepeatMode,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::ClientToBridgeMsgData)]
pub enum ClientToBridgeInteractionMsg {
  #[bridge_command]
  PhoneAnswer,
  #[bridge_command]
  PhoneDecline,
  #[bridge_command]
  PhoneCallImage(PhoneCallImage),
  #[bridge_command]
  PhoneCallMessage(PhoneCallMessage),
  #[bridge_command]
  IncreaseVolume,
  #[bridge_command]
  DecreaseVolume,
  #[bridge_command]
  MuteToggle,
  #[bridge_command]
  SkipToIndex(SkipToIndex),
  #[bridge_command]
  SkipNext,
  #[bridge_command]
  SkipPrev,
  #[bridge_command]
  SeekTo(SeekTo),
  #[bridge_command]
  Pause,
  #[bridge_command]
  Resume,
  #[bridge_command]
  SetShuffle(SetShuffle),
  #[bridge_command]
  SetRepeat(SetRepeat),
}
