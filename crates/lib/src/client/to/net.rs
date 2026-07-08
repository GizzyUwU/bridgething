use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::{NetError, NetFetchResponse, StreamBegin, StreamChunk, StreamEnd, StreamError, WsError, WsFrame};

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Response to `net.fetch`.
pub struct NetFetchReply {
  pub response: NetFetchResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Error response to `net.fetch`.
pub struct NetFetchErrorReply {
  pub error: NetError,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Response to `net.ws.open`.
pub struct NetWsOpenReply {
  /// The subprotocol the server selected, when `NetWsOpen.protocols`
  /// offered a list.
  pub accepted_protocol: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Error response to `net.ws.open`.
pub struct NetWsErrorReply {
  pub error: WsError,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// One inbound frame on an open WebSocket.
pub struct NetWsMessage {
  #[ts(type = "string")]
  pub connection_id: Uuid,
  pub frame: WsFrame,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// The WebSocket closed, locally or by the remote end.
pub struct NetWsClosed {
  #[ts(type = "string")]
  pub connection_id: Uuid,
  pub code: u16,
  pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Asynchronous WebSocket failure after `net.ws.open` already
/// succeeded.
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
/// Daemon -> webapp network surface: replies to `fetch` and `ws.open`,
/// WebSocket frame/close/error events, and the `Stream*` event trio
/// driven by `net.stream.open`.
pub enum BridgeToClientNetMsg {
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
