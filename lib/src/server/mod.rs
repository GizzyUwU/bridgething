use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

mod bluetooth;
mod interaction;
mod player;
mod storage;
mod system;

pub use bluetooth::*;
pub use interaction::*;
pub use player::*;
pub use storage::*;
pub use system::*;

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "server.ts")]
pub enum ServerEventType {
  Request,
  Response,
  Info,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "server.ts")]
pub struct ServerEvent {
  pub id: Uuid,
  #[serde(flatten)]
  pub data: ServerEventData,
  pub meta: ServerEventType,
  pub stock_msg_id: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "server.ts")]
pub enum ServerEventData {
  Bluetooth(ServerBluetoothEvent),
  Storage(ServerStorageEvent),
  System(ServerSystemEvent),
  Interaction(ServerInteractionEvent),
  Player(ServerPlayerEvent),
  Ack,
}

impl From<ServerBluetoothEvent> for ServerEventData {
  fn from(val: ServerBluetoothEvent) -> Self {
    ServerEventData::Bluetooth(val)
  }
}

impl From<ServerStorageEvent> for ServerEventData {
  fn from(val: ServerStorageEvent) -> Self {
    ServerEventData::Storage(val)
  }
}

impl From<ServerSystemEvent> for ServerEventData {
  fn from(val: ServerSystemEvent) -> Self {
    ServerEventData::System(val)
  }
}

impl From<ServerInteractionEvent> for ServerEventData {
  fn from(val: ServerInteractionEvent) -> Self {
    ServerEventData::Interaction(val)
  }
}

impl From<ServerPlayerEvent> for ServerEventData {
  fn from(val: ServerPlayerEvent) -> Self {
    ServerEventData::Player(val)
  }
}
