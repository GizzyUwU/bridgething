use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

mod from;
mod to;

pub use from::*;
pub use to::*;

use crate::{BridgeThingMeta, ForwardMessage};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "meta", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum GatewayMsgMeta {
  Command,
  Event,
  Request,
  Response {
    #[ts(type = "string")]
    request_id: Uuid,
  },
}

/// gateway -> bridgething
/// messages from the gateway (mobile or desktop app) to bridgething.
///
/// these messages will pass over bluetooth.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "gateway.ts")]
pub struct GatewayToBridgeMsg {
  #[ts(type = "string")]
  pub id: Uuid,
  #[serde(flatten)]
  pub meta: GatewayMsgMeta,
  #[serde(flatten)]
  pub data: GatewayToBridgeMsgData,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(
  tag = "type",
  content = "data",
  rename_all = "camelCase",
  rename_all_fields = "camelCase"
)]
#[ts(export, export_to = "gateway.ts")]
pub enum GatewayToBridgeMsgData {
  Version { version: String, app: String }, // event, response?

  File(GatewayToBridgeFileMsg),
  Chrome(GatewayToBridgeChromeMsg),

  // arbitrary data
  Forward(ForwardMessage), // request, response, event, command?
}

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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "gateway.ts")]
pub struct BridgeToGatewayMsg {
  #[ts(type = "string")]
  pub id: Uuid,
  #[serde(flatten)]
  pub meta: GatewayMsgMeta,
  #[serde(flatten)]
  pub data: BridgeToGatewayMsgData,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[allow(clippy::large_enum_variant)] // TODO: maybe remove this allow later
pub enum BridgeToGatewayMsgData {
  Ack,                      // response, happens when a command has been received and won't have a completion
  Done,                     // response, happens when a command has been completed
  Version(BridgeThingMeta), // event, response?

  File(BridgeToGatewayFileMsg),

  // arbitrary data
  Forward(ForwardMessage), // request, response, event, command?
}
