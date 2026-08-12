//! Wire-protocol primitives shared across the bridgething daemon's two
//! transport surfaces:
//!
//! - **Gateway** (Bluetooth, msgpack+gzip-framed): companion <-> daemon over
//!   RFCOMM, BLE, or iAP2 EA. Pair: `BridgeToGatewayMsgData` (daemon -> companion)
//!   and `GatewayToBridgeMsgData` (companion -> daemon).
//! - **Client** (local WebSocket, JSON): on-device webapp <-> daemon. Pair:
//!   `BridgeToClientMsgData` (daemon -> webapp) and `ClientToBridgeMsgData`
//!   (webapp -> daemon).

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// Correlation handle the responder echoes back so the requester's pending future can resolve.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "wire.ts")]
pub struct ResponseMeta {
  #[ts(type = "string")]
  pub request_id: Uuid,
}

/// Intent the sender signals for each message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "kind", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "wire.ts")]
pub enum MsgMeta {
  Command,
  Event,
  Request,
  Response(ResponseMeta),
}

/// Protocol-level failure the responder ships when a request could not be reached or dispatched
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "wire.ts")]
pub enum WireError {
  /// Receiver does not recognize this request variant, or recognizes it but cannot map the request to any operation it serves
  Unsupported,
  /// Receiver recognizes the variant but the backend is not yet wired
  Unimplemented,
  /// Receiver could not decode or validate the request payload
  Malformed { reason: String },
  /// Unexpected internal error while handling the request
  HandlerFailed { reason: String },
}

/// Failure modes a typed request can surface to the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestError<E> {
  Domain(E),
  Protocol(WireError),
  ResponseMismatch,
}

/// A fire-and-forget event the sender ships to the receiver
pub trait WireEvent<W>: Into<W> {}

/// A fire-and-forget command the receiver is expected to action
pub trait WireCommand<W>: Into<W> {}

/// A typed request whose response shape is statically known
pub trait WireRequest: Sized + Into<Self::Outbound> {
  type Outbound;
  type Inbound;
  type Response;
  type DomainError;

  fn extract(data: Self::Inbound) -> Result<Self::Response, RequestError<Self::DomainError>>;
  fn encode_response(response: Self::Response) -> Self::Inbound;
  fn encode_domain_error(err: Self::DomainError) -> Self::Inbound;
}
