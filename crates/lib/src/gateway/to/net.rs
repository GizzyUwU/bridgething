use bridgething_macros::{BridgeEnum, WireRequest};
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::{HttpHeader, NetFetchRequest, WsFrame};

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = BridgeToGateway,
  surface = Net,
  request_variant = Fetch,
  response = crate::gateway::NetFetchReply,
  response_variant = FetchReply,
  error = crate::gateway::NetFetchErrorReply,
  error_variant = FetchErrorReply,
)]
pub struct NetFetchRequestMsg {
  pub request: NetFetchRequest,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[wire_request(
  direction = BridgeToGateway,
  surface = Net,
  request_variant = WsOpen,
  response = crate::gateway::NetWsOpenReply,
  response_variant = WsOpenReply,
  error = crate::gateway::NetWsErrorReply,
  error_variant = WsErrorReply,
)]
pub struct NetWsOpen {
  #[ts(type = "string")]
  pub connection_id: Uuid,
  pub url: String,
  pub protocols: Option<Vec<String>>,
  pub headers: Option<Vec<HttpHeader>>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct NetWsClose {
  #[ts(type = "string")]
  pub connection_id: Uuid,
  pub code: Option<u16>,
  pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct NetWsSend {
  #[ts(type = "string")]
  pub connection_id: Uuid,
  pub frame: WsFrame,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct NetStreamOpen {
  #[ts(type = "string")]
  pub stream_id: Uuid,
  pub request: NetFetchRequest,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct NetStreamCancel {
  #[ts(type = "string")]
  pub stream_id: Uuid,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::BridgeToGatewayMsgData)]
pub enum BridgeToGatewayNetMsg {
  #[bridge_request]
  Fetch(NetFetchRequestMsg),
  #[bridge_request]
  WsOpen(NetWsOpen),
  #[bridge_command]
  WsClose(NetWsClose),
  #[bridge_command]
  WsSend(NetWsSend),
  #[bridge_command]
  StreamOpen(NetStreamOpen),
  #[bridge_command]
  StreamCancel(NetStreamCancel),
}
