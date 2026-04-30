//! Typed request/response pairing for the gateway protocol.
//!
//! `GatewayRequest` captures the gateway → bridge direction (gateway sends a
//! request, bridge responds). `BridgeRequest` is the mirror for bridge →
//! gateway. Each ties a request type to its response type and (optionally) a
//! domain-error type, so callers can `client.call(req).await -> Response` and
//! handlers can `respond_to::<Req>(response)` without naming wire-variant
//! constructors at every site.
//!
//! Implementations are produced by the `impl_gateway_request!` and
//! `impl_bridge_request!` macros (see crate root). Each invocation describes
//! the variant chain in one place: how to wrap a request for the wire, how to
//! narrow an inbound response into the typed payload, and how to encode a
//! response or domain error from a handler.

use crate::gateway::{BridgeToGatewayMsgData, GatewayError, GatewayToBridgeMsgData};

/// Failure modes a typed gateway request can surface to the caller.
///
/// `Domain` is the request's own error catalog (predictable, op-specific
/// failures). `Protocol` is the universal `GatewayError` (request unsupported,
/// payload malformed, handler hit an unexpected internal error).
/// `ResponseMismatch` means the wire shape did not match what the request
/// declared as its response — almost always a bug on one side.
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

/// Generate `GatewayRequest` impl + `From<Req> for GatewayToBridgeMsgData`.
///
/// Two forms: with-domain-error (six named sections) and without (three named
/// sections; sets `DomainError = Infallible` and gives `encode_domain_error`
/// an unreachable body).
///
/// ```ignore
/// impl_gateway_request! {
///   request: WebappSwitchTo,
///   response: WebappActive,
///   error: WebappError,
///   encode_request:
///     r => GatewayToBridgeMsgData::Webapp(GatewayToBridgeWebappMsg::SwitchTo(r)),
///   extract_response:
///     BridgeToGatewayMsgData::Webapp(BridgeToGatewayWebappMsg::Switched(v)) => v,
///   encode_response:
///     v => BridgeToGatewayMsgData::Webapp(BridgeToGatewayWebappMsg::Switched(v)),
///   extract_error:
///     BridgeToGatewayMsgData::Webapp(BridgeToGatewayWebappMsg::WebappError(e)) => e,
///   encode_error:
///     e => BridgeToGatewayMsgData::Webapp(BridgeToGatewayWebappMsg::WebappError(e)),
/// }
/// ```
#[macro_export]
macro_rules! impl_gateway_request {
  (
    request: $req:ty,
    response: $resp:ty,
    error: $err:ty,
    encode_request: $req_id:ident => $req_wrap:expr,
    extract_response: $resp_pat:pat => $resp_extract:expr,
    encode_response: $resp_id:ident => $resp_wrap:expr,
    extract_error: $err_pat:pat => $err_extract:expr,
    encode_error: $err_id:ident => $err_wrap:expr $(,)?
  ) => {
    impl $crate::gateway::GatewayRequest for $req {
      type Response = $resp;
      type DomainError = $err;

      fn extract(
        data: $crate::gateway::BridgeToGatewayMsgData,
      ) -> ::core::result::Result<Self::Response, $crate::gateway::RequestError<Self::DomainError>> {
        match data {
          $resp_pat => ::core::result::Result::Ok($resp_extract),
          $err_pat => ::core::result::Result::Err($crate::gateway::RequestError::Domain($err_extract)),
          $crate::gateway::BridgeToGatewayMsgData::Error(e) => {
            ::core::result::Result::Err($crate::gateway::RequestError::Protocol(e))
          }
          _ => ::core::result::Result::Err($crate::gateway::RequestError::ResponseMismatch),
        }
      }

      fn encode_response($resp_id: Self::Response) -> $crate::gateway::BridgeToGatewayMsgData {
        $resp_wrap
      }

      fn encode_domain_error($err_id: Self::DomainError) -> $crate::gateway::BridgeToGatewayMsgData {
        $err_wrap
      }
    }

    impl ::core::convert::From<$req> for $crate::gateway::GatewayToBridgeMsgData {
      fn from($req_id: $req) -> Self {
        $req_wrap
      }
    }
  };

  (
    request: $req:ty,
    response: $resp:ty,
    encode_request: $req_id:ident => $req_wrap:expr,
    extract_response: $resp_pat:pat => $resp_extract:expr,
    encode_response: $resp_id:ident => $resp_wrap:expr $(,)?
  ) => {
    impl $crate::gateway::GatewayRequest for $req {
      type Response = $resp;
      type DomainError = ::core::convert::Infallible;

      fn extract(
        data: $crate::gateway::BridgeToGatewayMsgData,
      ) -> ::core::result::Result<Self::Response, $crate::gateway::RequestError<Self::DomainError>> {
        match data {
          $resp_pat => ::core::result::Result::Ok($resp_extract),
          $crate::gateway::BridgeToGatewayMsgData::Error(e) => {
            ::core::result::Result::Err($crate::gateway::RequestError::Protocol(e))
          }
          _ => ::core::result::Result::Err($crate::gateway::RequestError::ResponseMismatch),
        }
      }

      fn encode_response($resp_id: Self::Response) -> $crate::gateway::BridgeToGatewayMsgData {
        $resp_wrap
      }

      fn encode_domain_error(err: Self::DomainError) -> $crate::gateway::BridgeToGatewayMsgData {
        match err {}
      }
    }

    impl ::core::convert::From<$req> for $crate::gateway::GatewayToBridgeMsgData {
      fn from($req_id: $req) -> Self {
        $req_wrap
      }
    }
  };
}

/// Mirror of `impl_gateway_request!` for the bridge → gateway direction.
/// Same shape, opposite outer types.
#[macro_export]
macro_rules! impl_bridge_request {
  (
    request: $req:ty,
    response: $resp:ty,
    error: $err:ty,
    encode_request: $req_id:ident => $req_wrap:expr,
    extract_response: $resp_pat:pat => $resp_extract:expr,
    encode_response: $resp_id:ident => $resp_wrap:expr,
    extract_error: $err_pat:pat => $err_extract:expr,
    encode_error: $err_id:ident => $err_wrap:expr $(,)?
  ) => {
    impl $crate::gateway::BridgeRequest for $req {
      type Response = $resp;
      type DomainError = $err;

      fn extract(
        data: $crate::gateway::GatewayToBridgeMsgData,
      ) -> ::core::result::Result<Self::Response, $crate::gateway::RequestError<Self::DomainError>> {
        match data {
          $resp_pat => ::core::result::Result::Ok($resp_extract),
          $err_pat => ::core::result::Result::Err($crate::gateway::RequestError::Domain($err_extract)),
          $crate::gateway::GatewayToBridgeMsgData::Error(e) => {
            ::core::result::Result::Err($crate::gateway::RequestError::Protocol(e))
          }
          _ => ::core::result::Result::Err($crate::gateway::RequestError::ResponseMismatch),
        }
      }

      fn encode_response($resp_id: Self::Response) -> $crate::gateway::GatewayToBridgeMsgData {
        $resp_wrap
      }

      fn encode_domain_error($err_id: Self::DomainError) -> $crate::gateway::GatewayToBridgeMsgData {
        $err_wrap
      }
    }

    impl ::core::convert::From<$req> for $crate::gateway::BridgeToGatewayMsgData {
      fn from($req_id: $req) -> Self {
        $req_wrap
      }
    }
  };

  (
    request: $req:ty,
    response: $resp:ty,
    encode_request: $req_id:ident => $req_wrap:expr,
    extract_response: $resp_pat:pat => $resp_extract:expr,
    encode_response: $resp_id:ident => $resp_wrap:expr $(,)?
  ) => {
    impl $crate::gateway::BridgeRequest for $req {
      type Response = $resp;
      type DomainError = ::core::convert::Infallible;

      fn extract(
        data: $crate::gateway::GatewayToBridgeMsgData,
      ) -> ::core::result::Result<Self::Response, $crate::gateway::RequestError<Self::DomainError>> {
        match data {
          $resp_pat => ::core::result::Result::Ok($resp_extract),
          $crate::gateway::GatewayToBridgeMsgData::Error(e) => {
            ::core::result::Result::Err($crate::gateway::RequestError::Protocol(e))
          }
          _ => ::core::result::Result::Err($crate::gateway::RequestError::ResponseMismatch),
        }
      }

      fn encode_response($resp_id: Self::Response) -> $crate::gateway::GatewayToBridgeMsgData {
        $resp_wrap
      }

      fn encode_domain_error(err: Self::DomainError) -> $crate::gateway::GatewayToBridgeMsgData {
        match err {}
      }
    }

    impl ::core::convert::From<$req> for $crate::gateway::BridgeToGatewayMsgData {
      fn from($req_id: $req) -> Self {
        $req_wrap
      }
    }
  };
}
