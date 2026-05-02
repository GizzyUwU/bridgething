use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::shared::RepeatMode;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(
  tag = "action",
  content = "args",
  rename_all = "camelCase",
  rename_all_fields = "camelCase"
)]
#[ts(export, export_to = "client.ts")]
pub enum ClientInteractionCommand {
  PhoneAnswer,
  PhoneDecline,
  PhoneCallImage { phone_number: String },
  PhoneCallMessage { phone_number: String, message: String },
  IncreaseVolume,
  DecreaseVolume,
  MuteToggle,
  SkipToIndex { index: u32 },
  SkipNext,
  SkipPrev,
  SeekTo { position_ms: u32 },
  Pause,
  Resume,
  SetShuffle { shuffle: bool },
  SetRepeat { repeat_mode: RepeatMode },
}
