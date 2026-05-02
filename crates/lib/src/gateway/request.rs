//! Typed request/response pairing for the gateway protocol.
//!
//! `GatewayRequest` captures the gateway → bridge direction (gateway
//! sends a request, bridge responds). `BridgeRequest` is the mirror for
//! bridge → gateway. Each ties a request type to its response type and
//! (optionally) a domain-error type, so callers can
//! `client.call(req).await -> Response` and handlers can
//! `respond_to::<Req>(response)` without naming wire-variant
//! constructors at every site.
//!
//! Implementations are produced by `#[derive(GatewayRequest)]` and
//! `#[derive(BridgeRequest)]` (see `bridgething-macros::request`). Each
//! derive consumes a `#[gateway_request(…)]` / `#[bridge_request(…)]`
//! attribute on the request payload type that names the surface, the
//! request variant, the response type/variant, and (optionally) the
//! domain-error type/variant. The derive emits the trait impl,
//! `From<Req> for <OuterMsgData>`, and a `const _: () = { … }` block
//! that compile-time-validates the response/error variants exist on
//! the cross-direction inner enum (via the hidden response-marker
//! module emitted by `#[derive(BridgeEnum)]`).

use crate::gateway::{BridgeToGatewayMsgData, GatewayError, GatewayToBridgeMsgData};

/// Failure modes a typed gateway request can surface to the caller.
///
/// `Domain` is the request's own error catalog (predictable, op-specific
/// failures). `Protocol` is the universal `GatewayError` (request
/// unsupported, payload malformed, handler hit an unexpected internal
/// error). `ResponseMismatch` means the wire shape did not match what
/// the request declared as its response — almost always a bug on one
/// side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestError<E> {
  Domain(E),
  Protocol(GatewayError),
  ResponseMismatch,
}

/// gateway → bridge: a typed request whose response shape is statically known.
pub trait GatewayRequest: Into<GatewayToBridgeMsgData> {
  type Response;
  type DomainError;

  fn extract(data: BridgeToGatewayMsgData) -> Result<Self::Response, RequestError<Self::DomainError>>;
  fn encode_response(response: Self::Response) -> BridgeToGatewayMsgData;
  fn encode_domain_error(err: Self::DomainError) -> BridgeToGatewayMsgData;
}

/// bridge → gateway: same shape as `GatewayRequest`, opposite directions.
pub trait BridgeRequest: Into<BridgeToGatewayMsgData> {
  type Response;
  type DomainError;

  fn extract(data: GatewayToBridgeMsgData) -> Result<Self::Response, RequestError<Self::DomainError>>;
  fn encode_response(response: Self::Response) -> GatewayToBridgeMsgData;
  fn encode_domain_error(err: Self::DomainError) -> GatewayToBridgeMsgData;
}
