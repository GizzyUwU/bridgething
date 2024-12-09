use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub mod bluetooth;
pub mod interaction;
pub mod storage;
pub mod system;
pub mod voice;

pub use bluetooth::*;
pub use interaction::*;
pub use storage::*;
pub use system::*;
pub use voice::*;

use super::RecvMsgData;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModernRecvMsg {
  pub id: Uuid,
  #[serde(flatten)]
  pub data: ModernRecvMsgType,
}

impl From<ModernRecvMsg> for RecvMsgData {
  fn from(msg: ModernRecvMsg) -> Self {
    match msg.data {
      ModernRecvMsgType::Bluetooth(msg) => Self::Bluetooth(msg),
      ModernRecvMsgType::Storage(msg) => Self::Storage(msg),
      ModernRecvMsgType::System(msg) => Self::System(msg),
      ModernRecvMsgType::Voice(msg) => Self::Voice(msg),
      ModernRecvMsgType::Interaction { msg, stock_msg_id } => Self::Interaction { msg, stock_msg_id },
    }
  }
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ModernRecvMsgType {
  Bluetooth(BluetoothRecv),
  Storage(StorageRecv),
  System(SystemRecv),
  Voice(VoiceRecv),
  Interaction {
    #[serde(flatten)]
    msg: InteractionRecv,
    stock_msg_id: Option<usize>,
  },
}
