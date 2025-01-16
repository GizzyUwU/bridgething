use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

mod bluetooth;
mod interaction;
mod storage;
mod system;
mod voice;

pub use bluetooth::*;
pub use interaction::*;
pub use storage::*;
pub use system::*;
pub use voice::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "client.ts")]
pub struct ClientCommand {
  pub id: Uuid,
  #[serde(flatten)]
  pub data: ClientCommandType,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub enum ClientCommandType {
  Bluetooth(ClientBluetoothCommand),
  Storage(ClientStorageCommand),
  System(ClientSystemCommand),
  Voice(ClientVoiceCommand),
  Interaction {
    #[serde(flatten)]
    msg: ClientInteractionCommand,
    stock_msg_id: Option<usize>,
  },
}
