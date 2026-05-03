use bridgething_macros::BridgeOuterEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

mod from;
mod to;

pub use from::*;
pub use to::*;

use crate::{
  BridgeThingMeta, ForwardMessage, GatewayMeta, NowPlayingUpdate,
  wire::{MsgMeta, WireError},
};

/// gateway -> bridgething
/// messages from the gateway (mobile or desktop app) to bridgething.
///
/// these messages will pass over bluetooth.
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "gateway.ts")]
pub struct GatewayToBridgeMsg {
  #[ts(type = "Uint8Array")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub id: Uuid,
  pub meta: MsgMeta,
  pub data: GatewayToBridgeMsgData,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeOuterEnum)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum GatewayToBridgeMsgData {
  #[from]
  Version(GatewayMeta),
  #[from]
  Asset(GatewayToBridgeAssetMsg),
  #[from]
  Authority(GatewayToBridgeAuthorityMsg),
  #[from]
  Chrome(GatewayToBridgeChromeMsg),
  #[from]
  System(GatewayToBridgeSystemMsg),
  #[from]
  Webapp(GatewayToBridgeWebappMsg),
  #[from]
  NowPlayingUpdate(NowPlayingUpdate),
  #[from]
  Error(WireError),
}

/// bridgething -> gateway
/// messages from bridgething to the gateway (mobile or desktop app).
///
/// these messages will pass over bluetooth.
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "gateway.ts")]
pub struct BridgeToGatewayMsg {
  #[ts(type = "Uint8Array")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub id: Uuid,
  pub meta: MsgMeta,
  pub data: BridgeToGatewayMsgData,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeOuterEnum)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum BridgeToGatewayMsgData {
  #[from]
  Version(Box<BridgeThingMeta>),
  #[from]
  Asset(BridgeToGatewayAssetMsg),
  #[from]
  System(BridgeToGatewaySystemMsg),
  #[from]
  Transport(BridgeToGatewayTransportMsg),
  #[from]
  Webapp(BridgeToGatewayWebappMsg),
  #[from]
  Forward(ForwardMessage),
  #[from]
  Error(WireError),
  /// response, command received and won't have a completion
  Ack,
  /// response, command has been completed
  Done,
}
