use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::{
  NetError, NetFetchResponse, NetFetchStreamBegin, NetFetchStreamChunk, NetFetchStreamEnd, WsError, WsFrame,
};

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct NetFetchReply {
  pub response: NetFetchResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct NetFetchErrorReply {
  pub error: NetError,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct NetWsOpenReply {
  #[ts(type = "string")]
  pub connection_id: Uuid,
  pub accepted_protocol: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct NetWsErrorReply {
  pub error: WsError,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct NetWsOpened {
  #[ts(type = "string")]
  pub connection_id: Uuid,
  pub accepted_protocol: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct NetWsMessage {
  #[ts(type = "string")]
  pub connection_id: Uuid,
  pub frame: WsFrame,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct NetWsClosed {
  #[ts(type = "string")]
  pub connection_id: Uuid,
  pub code: u16,
  pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct NetWsErrorEvent {
  #[ts(type = "string")]
  pub connection_id: Uuid,
  pub error: WsError,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
pub enum BridgeToClientNetMsg {
  #[bridge_response]
  FetchReply(NetFetchReply),
  #[bridge_response]
  FetchErrorReply(NetFetchErrorReply),
  #[bridge_event]
  FetchStreamBegin(NetFetchStreamBegin),
  #[bridge_event]
  FetchStreamChunk(NetFetchStreamChunk),
  #[bridge_event]
  FetchStreamEnd(NetFetchStreamEnd),
  #[bridge_response]
  WsOpenReply(NetWsOpenReply),
  #[bridge_response]
  WsErrorReply(NetWsErrorReply),
  #[bridge_event]
  WsOpened(NetWsOpened),
  #[bridge_event]
  WsMessage(NetWsMessage),
  #[bridge_event]
  WsClosed(NetWsClosed),
  #[bridge_event]
  WsErrorEvent(NetWsErrorEvent),
}
