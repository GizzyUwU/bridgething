use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

mod from;
mod to;

pub use from::*;
pub use to::*;

use crate::{BridgeThingMeta, ForwardMessage, GatewayMeta};

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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum GatewayToBridgeMsgData {
  Version(GatewayMeta), // event, response?

  File(GatewayToBridgeFileMsg),
  Chrome(GatewayToBridgeChromeMsg),
  Webapp(GatewayToBridgeWebappMsg),

  // arbitrary data
  Forward(ForwardMessage), // request, response, event, command?
}

#[typeshare]
#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct BridgeFile {
  pub path: String,
  #[serde_as(as = "serde_with::Bytes")]
  #[ts(type = "Uint8Array")]
  pub data: Vec<u8>,
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[allow(clippy::large_enum_variant)] // TODO: maybe remove this allow later
pub enum BridgeToGatewayMsgData {
  Version(BridgeThingMeta), // event, response?
  File(BridgeToGatewayFileMsg),
  Webapp(BridgeToGatewayWebappMsg),

  // arbitrary data
  Forward(ForwardMessage), // request, response, event, command?

  // acknowledgements
  Ack,  // response, happens when a command has been received and won't have a completion
  Nack, // response, happens when a command has been received but will not be processed
  Done, // response, happens when a command has been completed
}

impl From<ForwardMessage> for BridgeToGatewayMsgData {
  fn from(msg: ForwardMessage) -> Self {
    Self::Forward(msg)
  }
}

impl From<ForwardMessage> for GatewayToBridgeMsgData {
  fn from(msg: ForwardMessage) -> Self {
    Self::Forward(msg)
  }
}
