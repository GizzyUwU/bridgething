//! Wire-protocol primitives shared across the bridgething daemon's two
//! transport surfaces:
//!
//! - **Gateway** (Bluetooth, msgpack+gzip-framed): companion ↔ daemon over
//!   RFCOMM, BLE, or iAP2 EA. Pair: `BridgeToGatewayMsgData` (daemon → companion)
//!   and `GatewayToBridgeMsgData` (companion → daemon).
//! - **Client** (local WebSocket, JSON): on-device webapp ↔ daemon. Pair:
//!   `BridgeToClientMsgData` (daemon → webapp) and `ClientToBridgeMsgData`
//!   (webapp → daemon).
//!
//! Both share a single message-meta vocabulary (`MsgMeta`), a single
//! protocol-error catalog (`WireError`), and a single set of marker traits
//! parameterized by the wire data type they map into.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

/// Correlation handle the responder echoes back so the requester's
/// pending future can resolve.
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "wire.ts")]
pub struct ResponseMeta {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub request_id: Uuid,
}

/// Intent the sender signals for each message. Lets the receiver know
/// whether to send back a typed response, treat it as a one-way command,
/// or pair it against a pending request.
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "kind", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "wire.ts")]
pub enum MsgMeta {
  Command,
  Event,
  Request,
  Response(ResponseMeta),
}

/// Protocol-level failure the responder ships when a request could not be
/// reached or dispatched. Carried by the `Error` variant on every
/// `*MsgData` enum.
///
/// Domain-level errors (predictable, op-specific failures the caller may
/// want to recover from) live inside the per-op response variant, not
/// here.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "wire.ts")]
pub enum WireError {
  /// Receiver does not recognize this request variant. Used by the codec
  /// layer's auto-nack on a typed-decode failure (the variant the sender
  /// names is not in the receiver's enum) and by handlers that explicitly
  /// reject something they cannot map.
  Unsupported,
  /// Receiver recognizes the variant but the backend is not yet wired.
  /// Distinct from `Unsupported` so SDK consumers can tell "you have the
  /// wrong daemon version" from "this surface ships in a future slice".
  Unimplemented,
  /// Receiver could not decode or validate the request payload.
  Malformed { reason: String },
  /// Unexpected internal error while handling the request.
  HandlerFailed { reason: String },
}

/// Failure modes a typed request can surface to the caller.
///
/// `Domain` is the request's own error catalog (predictable, op-specific
/// failures). `Protocol` is the universal `WireError` (request unsupported,
/// payload malformed, handler hit an unexpected internal error).
/// `ResponseMismatch` means the wire shape did not match what the request
/// declared as its response — almost always a bug on one side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestError<E> {
  Domain(E),
  Protocol(WireError),
  ResponseMismatch,
}

/// A fire-and-forget event the sender ships to the receiver. The receiver
/// does whatever it wants with it; no reply is part of the contract.
///
/// The `W` type parameter is the wire data type the event lifts into,
/// e.g. `BridgeToGatewayMsgData` for daemon → companion or
/// `BridgeToClientMsgData` for daemon → webapp. Daemon-side typed-send
/// surfaces use the bound to type-check the call site.
pub trait WireEvent<W>: Into<W> {}

/// A fire-and-forget command the receiver is expected to action. Wire
/// `meta = command`; receiver may Ack to confirm receipt but no typed
/// response is part of the contract.
pub trait WireCommand<W>: Into<W> {}

/// A fire-and-forget event/command that must be addressed to a specific
/// peer. Pair with `WireEvent<W>` or `WireCommand<W>`. Without this marker,
/// codegen treats the variant as broadcastable and (for protocols that
/// support broadcast) omits the deviceId param at the surface-level send.
pub trait WireUnicast<W>: Into<W> {}

/// A typed request whose response shape is statically known. `Outbound`
/// is the wire data type the request lifts into; `Inbound` is the wire
/// data type the response arrives on (the opposite direction).
///
/// Implementations are produced by `#[derive(WireRequest)]` keyed off
/// `#[wire_request(...)]` (see `bridgething-macros::request`).
pub trait WireRequest: Sized + Into<Self::Outbound> {
  type Outbound;
  type Inbound;
  type Response;
  type DomainError;

  fn extract(data: Self::Inbound) -> Result<Self::Response, RequestError<Self::DomainError>>;
  fn encode_response(response: Self::Response) -> Self::Inbound;
  fn encode_domain_error(err: Self::DomainError) -> Self::Inbound;
}
