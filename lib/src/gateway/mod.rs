use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

mod from;
mod to;

pub use from::*;
pub use to::*;

/// gateway -> bridgething
/// messages from the gateway (mobile or desktop app) to bridgething.
///
/// these messages will pass over bluetooth.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "gateway.ts")]
pub struct GatewayToBridgeMsg {
  #[serde(with = "uuid::serde::simple")]
  #[ts(type = "string")]
  pub id: Uuid,
  #[serde(flatten)]
  pub data: GatewayToBridgeMsgType,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum GatewayToBridgeMsgType {
  Version { gateway: String, app: String },
  Command(GatewayToBridgeCommand),
  Event(GatewayToBridgeEvent),
  Request(GatewayToBridgeRequest),
  Response(GatewayToBridgeResponse),
}

/// bridgething -> gateway
/// messages from bridgething to the gateway (mobile or desktop app).
///
/// these messages will pass over bluetooth.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "gateway.ts")]
pub struct BridgeToGatewayMsg {
  #[serde(with = "uuid::serde::simple")]
  #[ts(type = "string")]
  pub id: Uuid,
  #[serde(flatten)]
  pub data: BridgeToGatewayMsgType,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum BridgeToGatewayMsgType {
  Version { bridgething: String, app: String },
  Command(BridgeToGatewayCommand),
  Event(BridgeToGatewayEvent),
  Request(BridgeToGatewayRequest),
  Response(BridgeToGatewayResponse),
}
