use serde::{Deserialize, Serialize};

use crate::msg::VoiceRecv;

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
