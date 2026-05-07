use bridgething_macros::BridgeEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

use crate::{NetError, NetFetchResponse, StreamBegin, StreamChunk, StreamEnd, StreamError, WsError, WsFrame};

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct NetFetchReply {
  pub response: NetFetchResponse,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct NetFetchErrorReply {
  pub error: NetError,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct NetWsOpenReply {
  pub accepted_protocol: Option<String>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct NetWsErrorReply {
  pub error: WsError,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct NetWsMessage {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub connection_id: Uuid,
  pub frame: WsFrame,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct NetWsClosed {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub connection_id: Uuid,
  pub code: u16,
  pub reason: String,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct NetWsErrorEvent {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub connection_id: Uuid,
  pub error: WsError,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::GatewayToBridgeMsgData)]
pub enum GatewayToBridgeNetMsg {
  #[bridge_response]
  FetchReply(NetFetchReply),
  #[bridge_response]
  FetchErrorReply(NetFetchErrorReply),
  #[bridge_response]
  WsOpenReply(NetWsOpenReply),
  #[bridge_response]
  WsErrorReply(NetWsErrorReply),
  #[bridge_event]
  WsMessage(NetWsMessage),
  #[bridge_event]
  WsClosed(NetWsClosed),
  #[bridge_event]
  WsErrorEvent(NetWsErrorEvent),
  #[bridge_event]
  StreamBegin(StreamBegin),
  #[bridge_event]
  StreamChunk(StreamChunk),
  #[bridge_event]
  StreamEnd(StreamEnd),
  #[bridge_event]
  StreamError(StreamError),
}
