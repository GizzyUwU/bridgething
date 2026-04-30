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

use crate::ForwardMessage;

/// Intent the webapp signals to the daemon for each `ClientCommand`. Mirrors
/// `GatewayMsgMeta` on the gateway side. Lets the SDK know whether to wait for
/// a paired response, and lets the daemon validate that requests it can't
/// satisfy surface as a typed `Error` rather than silent.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub enum ClientMsgMeta {
  /// Webapp expects exactly one response correlated by `id`.
  Request,
  /// Fire-and-forget: webapp does not wait for a response.
  Command,
  /// Webapp-emitted event with no expected response.
  Event,
}

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
  pub meta: ClientMsgMeta,
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
  Forward(ForwardMessage),

  // legacy and stock app stuffs
  #[ts(skip)]
  LegacyStock(ClientLegacyStockCommand),
}
