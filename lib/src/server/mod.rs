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

use crate::ForwardMessage;

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "meta", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export, export_to = "server.ts")]
pub enum ServerEventType {
  Request,
  Response { request_id: Uuid },
  Info,
}

/// bridgething -> client
/// messages from bridgething to the client (usually webpage) on the car thing.
///
/// these messages will pass through a websocket.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "server.ts", rename_all = "camelCase")]
pub struct ServerEvent {
  #[ts(type = "string")]
  pub id: Uuid,
  #[serde(flatten)]
  pub data: ServerEventData,
  #[serde(flatten)]
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
  Forward(ForwardMessage),
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
