use bridgething_macros::{BridgeEnum, WireRequest};
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

use crate::{TunnelClosed, TunnelData};

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = BridgeToGateway,
  surface = Tunnel,
  request_variant = Open,
  response = crate::gateway::TunnelOpenReply,
  response_variant = OpenReply,
  error = crate::gateway::TunnelErrorReply,
  error_variant = ErrorReply,
)]
pub struct TunnelOpen {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub tunnel_id: Uuid,
  pub host: String,
  pub port: u16,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::BridgeToGatewayMsgData)]
pub enum BridgeToGatewayTunnelMsg {
  #[bridge_request]
  Open(TunnelOpen),
  #[bridge_command]
  Data(TunnelData),
  #[bridge_command]
  Close(TunnelClosed),
}
