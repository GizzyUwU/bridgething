//! Net surface - webapp HTTP / WebSocket / Stream access proxied
//! through the connected companion. The Car Thing has no network of
//! its own; the daemon is a routing layer between webapp surfaces and
//! the companion's network stack. TLS terminates at the gateway; the
//! Bluetooth wire is plaintext.
//!
//! Three flows live here. **Fetch** is a typed request/response: the
//! webapp asks for a URL, the gateway does the work, the response
//! lands in one frame. Bulk priority on both legs keeps it from
//! starving normal-lane traffic. **WebSocket** carries a `connection_id`
//! the webapp's SDK assigns up front so reverse-direction event
//! routing (server -> webapp) is set up before the companion's ack
//! arrives. **Stream** is a unidirectional command + event flow for
//! cases where the webapp wants bytes incrementally as they arrive
//! (video, large media, server-sent events).

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "UPPERCASE")]
#[ts(export, export_to = "shared.ts")]
pub enum HttpMethod {
  Get,
  Head,
  Post,
  Put,
  Patch,
  Delete,
  Options,
}

/// One header on an HTTP request or response. Key order is preserved
/// across serialize/deserialize.
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct HttpHeader {
  pub name: String,
  pub value: String,
}

#[typeshare]
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum RedirectPolicy {
  /// Follow up to a gateway-defined cap (typically 5).
  #[default]
  Follow,
  /// Surface the redirect status to the caller; do not follow.
  Manual,
  /// Treat any 3xx as an error.
  Error,
}

#[typeshare]
#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct NetFetchRequest {
  pub url: String,
  pub method: HttpMethod,
  pub headers: Vec<HttpHeader>,
  #[serde_as(as = "Option<serde_with::Bytes>")]
  #[ts(type = "Uint8Array | null")]
  pub body: Option<Vec<u8>>,
  pub timeout_ms: Option<u32>,
  pub redirect: RedirectPolicy,
}

#[typeshare]
#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct NetFetchResponse {
  pub status: u16,
  pub headers: Vec<HttpHeader>,
  #[serde_as(as = "serde_with::Bytes")]
  #[ts(type = "Uint8Array")]
  pub body: Vec<u8>,
}

/// First event of an open stream. Carries the response status, headers,
/// and (when known) total payload size so the consumer can preallocate
/// or display progress. Subsequent `StreamChunk` and `StreamEnd` events
/// for the same `stream_id` follow.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct StreamBegin {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub stream_id: Uuid,
  pub status: u16,
  pub headers: Vec<HttpHeader>,
  pub total_size: Option<u32>,
}

/// One body chunk. Chunks arrive in order; `offset` is the byte
/// position of `bytes[0]` within the full body.
#[typeshare]
#[serde_with::serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct StreamChunk {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub stream_id: Uuid,
  pub offset: u32,
  #[serde_as(as = "serde_with::Bytes")]
  #[ts(type = "Uint8Array")]
  pub bytes: Vec<u8>,
}

/// Terminates a stream. After `End` no further chunks for `stream_id`
/// are valid and the daemon clears its routing entry.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct StreamEnd {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub stream_id: Uuid,
}

/// Stream failed mid-flight (or before the first byte). Terminal - the
/// daemon clears its routing entry. The `error` shape is shared with
/// `fetch` since the failure modes are identical.
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct StreamError {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub stream_id: Uuid,
  pub error: NetError,
}

#[typeshare]
#[serde_with::serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum WsFrame {
  Text(String),
  Binary(
    #[serde_as(as = "serde_with::Bytes")]
    #[ts(type = "Uint8Array")]
    Vec<u8>,
  ),
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum WsError {
  ConnectFailed { reason: String },
  FrameTooLarge,
  GatewayDisconnected,
  ProtocolError { reason: String },
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum NetError {
  RequestFailed { reason: String },
  Timeout,
  Unavailable,
  NoGateway,
}
