//! Net surface — webapp HTTP / WebSocket access proxied through the
//! connected companion. Inline-only response bodies up to 16 KB; larger
//! responses use the `NetFetchStream*` chunk shape on the Bulk lane.
//! TLS terminates at the gateway; the wire is plaintext over BT.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

/// Single-frame inline payload cap. Daemon-side fetch returns inline body
/// only when the response is at most this size; larger bodies switch to
/// `NetFetchStreamBegin`/`Chunk`/`End` on the Bulk lane.
pub const NET_FETCH_INLINE_MAX_BYTES: usize = 16 * 1024;

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

/// Outbound HTTP request. `body` is inline; webapps pushing large bodies
/// should chunk-stream via `AssetCache` and pass an asset id in a future
/// extension. `timeout_ms` defaults to gateway choice when None.
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

/// Inline response. Used when `body.len() <= NET_FETCH_INLINE_MAX_BYTES`;
/// otherwise the response arrives as `NetFetchStreamBegin/Chunk/End`.
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

/// First frame of a streamed response. `total_size` is `Content-Length`
/// when the gateway has it; otherwise `None` and the consumer accumulates
/// chunks until `End`.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct NetFetchStreamBegin {
  #[ts(type = "Uint8Array")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub request_id: Uuid,
  pub status: u16,
  pub headers: Vec<HttpHeader>,
  pub total_size: Option<u32>,
}

/// One body chunk of a streamed response. Chunks arrive in order;
/// `offset` is the byte position of `bytes[0]` within the full body.
#[typeshare]
#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct NetFetchStreamChunk {
  #[ts(type = "Uint8Array")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub request_id: Uuid,
  pub offset: u32,
  #[serde_as(as = "serde_with::Bytes")]
  #[ts(type = "Uint8Array")]
  pub bytes: Vec<u8>,
}

/// Terminates a stream. After `End` no further chunks for `request_id`
/// are valid.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct NetFetchStreamEnd {
  #[ts(type = "Uint8Array")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub request_id: Uuid,
}

/// One WS frame. The daemon does not split or merge frames.
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
  /// Gateway-side connect failed (DNS, TLS, refused).
  ConnectFailed { reason: String },
  /// Per-connection backpressure cap (64 frames or 1 MB) hit.
  Backpressure,
  /// Frame exceeded the per-connection 16 KB cap.
  FrameTooLarge,
  /// The companion went away while the WS was open.
  GatewayDisconnected,
  /// Server-side WS protocol violation surfaced by the underlying lib.
  ProtocolError { reason: String },
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum NetError {
  /// Gateway-side request failed before headers were received (DNS, TLS,
  /// connect refused, transport hiccup).
  RequestFailed { reason: String },
  /// `timeout_ms` elapsed before headers were received.
  Timeout,
  /// Gateway is connected but the Net surface is unavailable
  /// (`SurfaceAvailability::net_fetch` is false).
  Unavailable,
  /// The companion is not connected.
  NoGateway,
}
