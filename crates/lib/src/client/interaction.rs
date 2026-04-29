use serde::{Deserialize, Serialize};
use ts_rs::TS;

// TODO: refactor this into more command types so not spotify-specific
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
  SkipToIndex { index: usize },
  SkipNext,
  SkipPrev { allow_seeking: bool },
  SeekTo { position: usize },
  Pause,
  Resume,
  SetShuffle { shuffle: bool },
  SetRepeat { repeat_mode: bool },
}
