use libbridgething::client::{ClientToBridgeVoiceMsgCommand, MicMute, MicUnmute};
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockVoiceErrorPayload {
  cause: String,
  domain: String,
}

impl From<ClientToBridgeVoiceMsgCommand> for StockVoiceRecv {
  fn from(data: ClientToBridgeVoiceMsgCommand) -> Self {
    match data {
      ClientToBridgeVoiceMsgCommand::Cancel => StockVoiceRecv::Cancel,
      ClientToBridgeVoiceMsgCommand::PushToTalk => StockVoiceRecv::PushToTalk,
      ClientToBridgeVoiceMsgCommand::MuteMic(MicMute { preserve }) => StockVoiceRecv::MuteMic {
        attributes: MuteStatusAttributes { preserve },
      },
      ClientToBridgeVoiceMsgCommand::UnmuteMic(MicUnmute { preserve }) => StockVoiceRecv::UnmuteMic {
        attributes: MuteStatusAttributes { preserve },
      },
    }
  }
}

impl From<StockVoiceRecv> for ClientToBridgeVoiceMsgCommand {
  fn from(data: StockVoiceRecv) -> Self {
    match data {
      StockVoiceRecv::Cancel => ClientToBridgeVoiceMsgCommand::Cancel,
      StockVoiceRecv::PushToTalk => ClientToBridgeVoiceMsgCommand::PushToTalk,
      StockVoiceRecv::MuteMic { attributes } => ClientToBridgeVoiceMsgCommand::MuteMic(MicMute {
        preserve: attributes.preserve,
      }),
      StockVoiceRecv::UnmuteMic { attributes } => ClientToBridgeVoiceMsgCommand::UnmuteMic(MicUnmute {
        preserve: attributes.preserve,
      }),
    }
  }
}
