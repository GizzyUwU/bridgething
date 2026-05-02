use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

mod error;
mod from;
mod request;
mod to;

pub use error::*;
pub use from::*;
pub use request::*;
pub use to::*;

use crate::{BridgeThingMeta, ForwardMessage, GatewayMeta, NowPlayingUpdate};

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct ResponseMeta {
  #[ts(type = "Uint8Array")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub request_id: Uuid,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "kind", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum GatewayMsgMeta {
  Command,
  Event,
  Request,
  Response(ResponseMeta),
}

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
  pub meta: GatewayMsgMeta,
  pub data: GatewayToBridgeMsgData,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, derive_more::From)]
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
  Webapp(GatewayToBridgeWebappMsg),
  #[from]
  Forward(ForwardMessage),
  #[from]
  NowPlayingUpdate(NowPlayingUpdate),
  #[from]
  Error(GatewayError),
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
  pub meta: GatewayMsgMeta,
  pub data: BridgeToGatewayMsgData,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, derive_more::From)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[allow(clippy::large_enum_variant)]
pub enum BridgeToGatewayMsgData {
  #[from]
  Version(BridgeThingMeta),
  #[from]
  Asset(BridgeToGatewayAssetMsg),
  #[from]
  Transport(BridgeToGatewayTransportMsg),
  #[from]
  Webapp(BridgeToGatewayWebappMsg),
  #[from]
  Forward(ForwardMessage),
  #[from]
  Error(GatewayError),
  /// response, command received and won't have a completion
  Ack,
  /// response, command has been completed
  Done,
}
