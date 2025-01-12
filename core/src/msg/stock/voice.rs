use serde::{Deserialize, Serialize};

use crate::msg::{PossibleSendMsg, VoiceRecv};

use super::StockSendMsg;

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
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StockLocalCommand {
  None,
  Play,
  Resume,
  Stop,
  Next,
  Previous,
  Mute,
}

impl From<StockVoiceSend> for StockSendMsg {
  fn from(val: StockVoiceSend) -> Self {
    Self::Voice(val)
  }
}

impl From<StockVoiceSend> for PossibleSendMsg {
  fn from(val: StockVoiceSend) -> Self {
    Self::Stock(StockSendMsg::Voice(val))
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockVoiceErrorPayload {
  cause: String,
  domain: String,
}

impl From<VoiceRecv> for StockVoiceRecv {
  fn from(data: VoiceRecv) -> Self {
    match data {
      VoiceRecv::Cancel => StockVoiceRecv::Cancel,
      VoiceRecv::PushToTalk => StockVoiceRecv::PushToTalk,
      VoiceRecv::MuteMic { preserve } => StockVoiceRecv::MuteMic {
        attributes: MuteStatusAttributes { preserve },
      },
      VoiceRecv::UnmuteMic { preserve } => StockVoiceRecv::UnmuteMic {
        attributes: MuteStatusAttributes { preserve },
      },
    }
  }
}

impl From<StockVoiceRecv> for VoiceRecv {
  fn from(data: StockVoiceRecv) -> Self {
    match data {
      StockVoiceRecv::Cancel => VoiceRecv::Cancel,
      StockVoiceRecv::PushToTalk => VoiceRecv::PushToTalk,
      StockVoiceRecv::MuteMic { attributes } => VoiceRecv::MuteMic {
        preserve: attributes.preserve,
      },
      StockVoiceRecv::UnmuteMic { attributes } => VoiceRecv::UnmuteMic {
        preserve: attributes.preserve,
      },
    }
  }
}
