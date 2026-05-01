use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

mod bluetooth;
mod interaction;
mod peer;
mod player;
mod storage;
mod system;

pub use bluetooth::*;
pub use interaction::*;
pub use peer::*;
pub use player::*;
pub use storage::*;
pub use system::*;

use crate::{ForwardMessage, gateway::GatewayError};

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "meta", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export, export_to = "server.ts")]
pub enum ServerEventType {
  Request,
  Response { request_id: Uuid },
  Event,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS, derive_more::From)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export, export_to = "server.ts")]
pub enum ServerEventData {
  #[from]
  Bluetooth(ServerBluetoothEvent),
  #[from]
  Storage(ServerStorageEvent),
  #[from]
  System(ServerSystemEvent),
  #[from]
  Interaction(ServerInteractionEvent),
  #[from]
  Player(ServerPlayerEvent),
  #[from]
  Peer(ServerPeerEvent),
  #[from]
  Forward(ForwardMessage),
  #[from]
  Error(GatewayError),
  Ack,
}
