use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::BridgeThingMeta;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "meta", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum GatewayMsgMeta {
  Command,
  Event,
  Request,
  Response {
    // #[serde(with = "uuid::serde::simple")]
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
  // #[serde(with = "uuid::serde::simple")]
  #[ts(type = "string")]
  pub id: Uuid,
  #[serde(flatten)]
  pub meta: GatewayMsgMeta,
  #[serde(flatten)]
  pub data: GatewayToBridgeMsgData,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum GatewayToBridgeMsgData {
  Version {
    version: String,
    app: String,
  }, // event, response?

  // files
  ListFiles, // request
  DeleteFiles {
    files: Vec<String>,
  }, // command
  AddFiles {
    #[debug(skip)]
    files: Vec<BridgeFile>,
  }, // command

  // chrome
  Navigate {
    url: String,
  }, // command

  // arbitrary data
  Data(ArbitraryData), // request, response, event, command?
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

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(untagged, rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum ArbitraryData {
  String(String),
  Bytes(#[ts(type = "Uint8Array")] Vec<u8>),
}

/// bridgething -> gateway
/// messages from bridgething to the gateway (mobile or desktop app).
///
/// these messages will pass over bluetooth.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "gateway.ts")]
pub struct BridgeToGatewayMsg {
  // #[serde(with = "uuid::serde::simple")]
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
  Version(BridgeThingMeta), // event, response?

  // files
  Files { files: Vec<String> }, // response

  // arbitrary data
  Data(ArbitraryData), // request, response, event, command?
}
