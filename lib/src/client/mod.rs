use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

mod bluetooth;
mod interaction;
mod stock;
mod store;
mod system;
mod voice;

pub use bluetooth::*;
pub use interaction::*;
pub use stock::*;
pub use store::*;
pub use system::*;
pub use voice::*;

/// client -> bridgething
/// messages from the client (usually webpage) on the car thing to the bridgething
/// daemon.
///
/// these messages will pass through a websocket.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "client.ts")]
pub struct ClientCommand {
  #[ts(type = "string")]
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
  Store(ClientKVStoreCommand),
  System(ClientSystemCommand),
  Voice(ClientVoiceCommand),
  Interaction(ClientInteractionCommand),
  // Forward(),

  // legacy and stock app stuffs
  #[ts(skip)]
  LegacyStock(ClientLegacyStockCommand),
}
