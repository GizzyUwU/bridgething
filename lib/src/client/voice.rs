use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "action", content = "args", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub enum ClientVoiceCommand {
  Cancel,
  PushToTalk,
  MuteMic { preserve: bool },
  UnmuteMic { preserve: bool },
}
