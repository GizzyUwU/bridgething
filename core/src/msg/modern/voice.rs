use serde::{Deserialize, Serialize};

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", content = "args", rename_all = "camelCase")]
pub enum VoiceRecv {
  Cancel,
  PushToTalk,
  MuteMic { preserve: bool },
  UnmuteMic { preserve: bool },
}
