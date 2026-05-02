use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

mod from;
mod to;

pub use from::*;
pub use to::*;

use crate::{
  ForwardMessage,
  wire::{MsgMeta, WireError},
};

/// client -> bridgething
/// messages from the client (webapp) to bridgething.
///
/// these messages travel over the local websocket on port 8891.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "client.ts")]
pub struct ClientToBridgeMsg {
  #[ts(type = "Uint8Array")]
  pub id: Uuid,
  pub meta: MsgMeta,
  pub data: ClientToBridgeMsgData,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, derive_more::From)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub enum ClientToBridgeMsgData {
  #[from]
  Asset(ClientToBridgeAssetMsg),
  #[from]
  Bluetooth(ClientToBridgeBluetoothMsg),
  #[from]
  Store(ClientToBridgeStoreMsg),
  #[from]
  System(ClientToBridgeSystemMsg),
  #[from]
  Voice(ClientToBridgeVoiceMsg),
  #[from]
  Interaction(ClientToBridgeInteractionMsg),
  #[from]
  Forward(ForwardMessage),

  // legacy and stock app stuffs
  #[from]
  #[ts(skip)]
  LegacyStock(ClientLegacyStockCommand),
}

/// bridgething -> client
/// messages from bridgething to the client (webapp).
///
/// these messages travel over the local websocket on port 8891.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "client.ts")]
pub struct BridgeToClientMsg {
  #[ts(type = "Uint8Array")]
  pub id: Uuid,
  pub meta: MsgMeta,
  pub data: BridgeToClientMsgData,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS, derive_more::From)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[allow(clippy::large_enum_variant)]
pub enum BridgeToClientMsgData {
  #[from]
  Asset(BridgeToClientAssetMsg),
  #[from]
  Bluetooth(BridgeToClientBluetoothMsg),
  #[from]
  Store(BridgeToClientStoreMsg),
  #[from]
  System(BridgeToClientSystemMsg),
  #[from]
  Interaction(BridgeToClientInteractionMsg),
  #[from]
  Player(BridgeToClientPlayerMsg),
  #[from]
  Peer(BridgeToClientPeerMsg),
  #[from]
  Forward(ForwardMessage),
  #[from]
  Error(WireError),
  /// response, command received and won't have a completion
  Ack,
  /// response, command has been completed
  Done,
}
