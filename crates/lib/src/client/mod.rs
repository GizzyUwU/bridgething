use bridgething_macros::BridgeOuterEnum;
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "client.ts")]
pub struct ClientToBridgeMsg {
  #[ts(type = "Uint8Array")]
  pub id: Uuid,
  pub meta: MsgMeta,
  pub data: ClientToBridgeMsgData,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS, BridgeOuterEnum)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub enum ClientToBridgeMsgData {
  #[from]
  Asset(ClientToBridgeAssetMsg),
  #[from]
  Audio(ClientToBridgeAudioMsg),
  #[from]
  Bluetooth(ClientToBridgeBluetoothMsg),
  #[from]
  Capabilities(ClientToBridgeCapabilitiesMsg),
  #[from]
  Config(ClientToBridgeConfigMsg),
  #[from]
  Geo(ClientToBridgeGeoMsg),
  #[from]
  Hardware(ClientToBridgeHardwareMsg),
  #[from]
  Library(ClientToBridgeLibraryMsg),
  #[from]
  Net(ClientToBridgeNetMsg),
  #[from]
  Notifications(ClientToBridgeNotificationsMsg),
  #[from]
  Phone(ClientToBridgePhoneMsg),
  #[from]
  Player(ClientToBridgePlayerMsg),
  #[from]
  Store(ClientToBridgeStoreMsg),
  #[from]
  System(ClientToBridgeSystemMsg),
  #[from]
  Time(ClientToBridgeTimeMsg),
  #[from]
  Voice(ClientToBridgeVoiceMsg),
  #[from]
  Webapp(ClientToBridgeWebappMsg),
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS, BridgeOuterEnum)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub enum BridgeToClientMsgData {
  #[from]
  Asset(BridgeToClientAssetMsg),
  #[from]
  Audio(BridgeToClientAudioMsg),
  #[from]
  Bluetooth(BridgeToClientBluetoothMsg),
  #[from]
  Capabilities(BridgeToClientCapabilitiesMsg),
  #[from]
  Config(BridgeToClientConfigMsg),
  #[from]
  Geo(BridgeToClientGeoMsg),
  #[from]
  Hardware(BridgeToClientHardwareMsg),
  #[from]
  Library(BridgeToClientLibraryMsg),
  #[from]
  Net(BridgeToClientNetMsg),
  #[from]
  Notifications(BridgeToClientNotificationsMsg),
  #[from]
  Peer(BridgeToClientPeerMsg),
  #[from]
  Phone(BridgeToClientPhoneMsg),
  #[from]
  Player(BridgeToClientPlayerMsg),
  #[from]
  Store(BridgeToClientStoreMsg),
  #[from]
  System(BridgeToClientSystemMsg),
  #[from]
  Time(BridgeToClientTimeMsg),
  #[from]
  Webapp(BridgeToClientWebappMsg),
  #[from]
  Forward(ForwardMessage),
  #[from]
  Error(WireError),
  /// response, command received and won't have a completion
  Ack,
  /// response, command has been completed
  Done,
}
