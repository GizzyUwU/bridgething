use bridgething_macros::{BridgeDispatch, BridgeEnum, WireRequest};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::{HttpHeader, NetFetchRequest, WsFrame};

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[wire_request(
  direction = ClientToBridge,
  surface = Net,
  request_variant = Fetch,
  response = crate::client::NetFetchReply,
  response_variant = FetchReply,
  error = crate::client::NetFetchErrorReply,
  error_variant = FetchErrorReply,
)]
/// Payload for `net.fetch`: a single proxied HTTP request/response
/// round-trip through the connected companion's network stack.
pub struct NetFetch {
  pub request: NetFetchRequest,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, WireRequest)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[wire_request(
  direction = ClientToBridge,
  surface = Net,
  request_variant = WsOpen,
  response = crate::client::NetWsOpenReply,
  response_variant = WsOpenReply,
  error = crate::client::NetWsErrorReply,
  error_variant = WsErrorReply,
)]
/// Payload for `net.ws.open`: establish a WebSocket routed through the
/// companion. `connection_id` is assigned by the webapp up front so
/// inbound frame/close/error routing is wired before the companion's
/// ack arrives.
pub struct NetWsOpen {
  #[ts(type = "string")]
  pub connection_id: Uuid,
  pub url: String,
  /// Subprotocols offered to the server, in preference order.
  pub protocols: Option<Vec<String>>,
  pub headers: Option<Vec<HttpHeader>>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for `net.ws.close`. Ignored by the daemon if `connection_id`
/// isn't owned by the calling webapp.
pub struct NetWsClose {
  #[ts(type = "string")]
  pub connection_id: Uuid,
  pub code: Option<u16>,
  pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for `net.ws.send`.
pub struct NetWsSend {
  #[ts(type = "string")]
  pub connection_id: Uuid,
  pub frame: WsFrame,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for `net.stream.open`: like `fetch` but delivers the
/// response body incrementally as `StreamChunk` events instead of one
/// frame. `stream_id` is assigned by the webapp.
pub struct NetStreamOpen {
  #[ts(type = "string")]
  pub stream_id: Uuid,
  pub request: NetFetchRequest,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
/// Payload for `net.stream.cancel`. Ignored by the daemon if
/// `stream_id` isn't owned by the calling webapp.
pub struct NetStreamCancel {
  #[ts(type = "string")]
  pub stream_id: Uuid,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum, BridgeDispatch)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::ClientToBridgeMsgData)]
/// Webapp -> daemon network surface: HTTP fetch, WebSocket, and byte
/// streams, all proxied through the connected companion. The device
/// has no network connectivity of its own.
pub enum ClientToBridgeNetMsg {
  #[bridge_request]
  Fetch(NetFetch),
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
