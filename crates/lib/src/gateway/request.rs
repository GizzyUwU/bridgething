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
//! the variant chain in structured form: a `surface` ident names the outer
//! variant of the wire enum (and, by convention, the suffix of the inner
//! enum name — `Webapp` -> `<Direction>WebappMsg`), and one ident per
//! request/response/error gives the inner-enum variant. The macro derives
//! the actual enum-constructor paths via the `paste` crate; codegen reads
//! the same structured idents to generate per-language SDK helpers.

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
/// Four arms cover the matrix of {tuple-request, unit-request} ×
/// {with-error, without-error}. The variant-ident syntax `SwitchTo(_)`
/// signals tuple (the variant carries a payload bound by the macro);
/// `List` (no parens) signals unit (the variant carries no value).
/// Response and error variants are always tuple-shaped in our protocol;
/// the `(_)` is required there too for syntactic regularity and so
/// codegen can parse all variant idents the same way.
///
/// ```ignore
/// impl_gateway_request! {
///   request: WebappSwitchTo,
///   surface: Webapp,
///   request_variant: SwitchTo(_),
///   response: WebappActive,
///   response_variant: Switched(_),
///   error: WebappError,
///   error_variant: WebappError(_),
/// }
/// ```
#[macro_export]
macro_rules! impl_gateway_request {
  // Tuple-request, with domain error.
  (
    request:
    $req:ty,surface:
    $surf:ident,request_variant:
    $rv:ident(_),response:
    $resp:ty,response_variant:
    $rspv:ident(_),error:
    $err:ty,error_variant:
    $ev:ident(_) $(,)?
  ) => {
    $crate::__impl_gateway_request_body! {
      request: $req,
      surface: $surf,
      request_ctor: $rv,
      request_value: payload,
      request_takes_payload: true,
      response: $resp,
      response_variant: $rspv,
      error_kind: with_error,
      error_type: $err,
      error_variant: $ev,
    }
  };

  // Tuple-request, no domain error.
  (
    request:
    $req:ty,surface:
    $surf:ident,request_variant:
    $rv:ident(_),response:
    $resp:ty,response_variant:
    $rspv:ident(_) $(,)?
  ) => {
    $crate::__impl_gateway_request_body! {
      request: $req,
      surface: $surf,
      request_ctor: $rv,
      request_value: payload,
      request_takes_payload: true,
      response: $resp,
      response_variant: $rspv,
      error_kind: without_error,
    }
  };

  // Unit-request, no domain error. Unit-request with error is unused
  // today; add an arm if/when it becomes needed.
  (
    request:
    $req:ty,surface:
    $surf:ident,request_variant:
    $rv:ident,response:
    $resp:ty,response_variant:
    $rspv:ident(_) $(,)?
  ) => {
    $crate::__impl_gateway_request_body! {
      request: $req,
      surface: $surf,
      request_ctor: $rv,
      request_value: payload,
      request_takes_payload: false,
      response: $resp,
      response_variant: $rspv,
      error_kind: without_error,
    }
  };
}

/// Internal expansion target for `impl_gateway_request!`. Centralizes the
/// `paste!`-driven enum-path construction so each public arm above only
/// describes what it captures.
#[macro_export]
#[doc(hidden)]
macro_rules! __impl_gateway_request_body {
  // With domain error.
  (
    request:
    $req:ty,surface:
    $surf:ident,request_ctor:
    $rv:ident,request_value:
    $rval:ident,request_takes_payload: true,response:
    $resp:ty,response_variant:
    $rspv:ident,error_kind: with_error,error_type:
    $err:ty,error_variant:
    $ev:ident $(,)?
  ) => {
    ::paste::paste! {
      impl $crate::gateway::GatewayRequest for $req {
        type Response = $resp;
        type DomainError = $err;

        fn extract(
          data: $crate::gateway::BridgeToGatewayMsgData,
        ) -> ::core::result::Result<Self::Response, $crate::gateway::RequestError<Self::DomainError>> {
          match data {
            $crate::gateway::BridgeToGatewayMsgData::$surf(
              $crate::gateway::[<BridgeToGateway $surf Msg>]::$rspv(v),
            ) => ::core::result::Result::Ok(v),
            $crate::gateway::BridgeToGatewayMsgData::$surf(
              $crate::gateway::[<BridgeToGateway $surf Msg>]::$ev(e),
            ) => ::core::result::Result::Err($crate::gateway::RequestError::Domain(e)),
            $crate::gateway::BridgeToGatewayMsgData::Error(e) => {
              ::core::result::Result::Err($crate::gateway::RequestError::Protocol(e))
            }
            _ => ::core::result::Result::Err($crate::gateway::RequestError::ResponseMismatch),
          }
        }

        fn encode_response(v: Self::Response) -> $crate::gateway::BridgeToGatewayMsgData {
          $crate::gateway::BridgeToGatewayMsgData::$surf(
            $crate::gateway::[<BridgeToGateway $surf Msg>]::$rspv(v),
          )
        }

        fn encode_domain_error(e: Self::DomainError) -> $crate::gateway::BridgeToGatewayMsgData {
          $crate::gateway::BridgeToGatewayMsgData::$surf(
            $crate::gateway::[<BridgeToGateway $surf Msg>]::$ev(e),
          )
        }
      }

      impl ::core::convert::From<$req> for $crate::gateway::GatewayToBridgeMsgData {
        fn from($rval: $req) -> Self {
          $crate::gateway::GatewayToBridgeMsgData::$surf(
            $crate::gateway::[<GatewayToBridge $surf Msg>]::$rv($rval),
          )
        }
      }

      impl $req {
        /// Returns true if `msg` is the typed-response or domain-error
        /// variant of this request. Used by upstream filters to drop
        /// stray response-shape arrivals that bypass the request-id
        /// match (timed-out, late, non-SDK senders).
        pub fn is_response_variant(
          msg: &$crate::gateway::[<BridgeToGateway $surf Msg>],
        ) -> bool {
          ::core::matches!(
            msg,
            $crate::gateway::[<BridgeToGateway $surf Msg>]::$rspv(_)
              | $crate::gateway::[<BridgeToGateway $surf Msg>]::$ev(_),
          )
        }
      }
    }
  };

  // Tuple-request, no domain error.
  (
    request:
    $req:ty,surface:
    $surf:ident,request_ctor:
    $rv:ident,request_value:
    $rval:ident,request_takes_payload: true,response:
    $resp:ty,response_variant:
    $rspv:ident,error_kind: without_error
    $(,)?
  ) => {
    ::paste::paste! {
      impl $crate::gateway::GatewayRequest for $req {
        type Response = $resp;
        type DomainError = ::core::convert::Infallible;

        fn extract(
          data: $crate::gateway::BridgeToGatewayMsgData,
        ) -> ::core::result::Result<Self::Response, $crate::gateway::RequestError<Self::DomainError>> {
          match data {
            $crate::gateway::BridgeToGatewayMsgData::$surf(
              $crate::gateway::[<BridgeToGateway $surf Msg>]::$rspv(v),
            ) => ::core::result::Result::Ok(v),
            $crate::gateway::BridgeToGatewayMsgData::Error(e) => {
              ::core::result::Result::Err($crate::gateway::RequestError::Protocol(e))
            }
            _ => ::core::result::Result::Err($crate::gateway::RequestError::ResponseMismatch),
          }
        }

        fn encode_response(v: Self::Response) -> $crate::gateway::BridgeToGatewayMsgData {
          $crate::gateway::BridgeToGatewayMsgData::$surf(
            $crate::gateway::[<BridgeToGateway $surf Msg>]::$rspv(v),
          )
        }

        fn encode_domain_error(err: Self::DomainError) -> $crate::gateway::BridgeToGatewayMsgData {
          match err {}
        }
      }

      impl ::core::convert::From<$req> for $crate::gateway::GatewayToBridgeMsgData {
        fn from($rval: $req) -> Self {
          $crate::gateway::GatewayToBridgeMsgData::$surf(
            $crate::gateway::[<GatewayToBridge $surf Msg>]::$rv($rval),
          )
        }
      }

      impl $req {
        pub fn is_response_variant(
          msg: &$crate::gateway::[<BridgeToGateway $surf Msg>],
        ) -> bool {
          ::core::matches!(
            msg,
            $crate::gateway::[<BridgeToGateway $surf Msg>]::$rspv(_),
          )
        }
      }
    }
  };

  // Unit-request, no domain error.
  (
    request:
    $req:ty,surface:
    $surf:ident,request_ctor:
    $rv:ident,request_value:
    $rval:ident,request_takes_payload: false,response:
    $resp:ty,response_variant:
    $rspv:ident,error_kind: without_error
    $(,)?
  ) => {
    ::paste::paste! {
      impl $crate::gateway::GatewayRequest for $req {
        type Response = $resp;
        type DomainError = ::core::convert::Infallible;

        fn extract(
          data: $crate::gateway::BridgeToGatewayMsgData,
        ) -> ::core::result::Result<Self::Response, $crate::gateway::RequestError<Self::DomainError>> {
          match data {
            $crate::gateway::BridgeToGatewayMsgData::$surf(
              $crate::gateway::[<BridgeToGateway $surf Msg>]::$rspv(v),
            ) => ::core::result::Result::Ok(v),
            $crate::gateway::BridgeToGatewayMsgData::Error(e) => {
              ::core::result::Result::Err($crate::gateway::RequestError::Protocol(e))
            }
            _ => ::core::result::Result::Err($crate::gateway::RequestError::ResponseMismatch),
          }
        }

        fn encode_response(v: Self::Response) -> $crate::gateway::BridgeToGatewayMsgData {
          $crate::gateway::BridgeToGatewayMsgData::$surf(
            $crate::gateway::[<BridgeToGateway $surf Msg>]::$rspv(v),
          )
        }

        fn encode_domain_error(err: Self::DomainError) -> $crate::gateway::BridgeToGatewayMsgData {
          match err {}
        }
      }

      impl ::core::convert::From<$req> for $crate::gateway::GatewayToBridgeMsgData {
        fn from(_: $req) -> Self {
          $crate::gateway::GatewayToBridgeMsgData::$surf(
            $crate::gateway::[<GatewayToBridge $surf Msg>]::$rv,
          )
        }
      }

      impl $req {
        pub fn is_response_variant(
          msg: &$crate::gateway::[<BridgeToGateway $surf Msg>],
        ) -> bool {
          ::core::matches!(
            msg,
            $crate::gateway::[<BridgeToGateway $surf Msg>]::$rspv(_),
          )
        }
      }
    }
  };
}

/// Mirror of `impl_gateway_request!` for the bridge → gateway direction.
/// Same shape, opposite outer types.
#[macro_export]
macro_rules! impl_bridge_request {
  // Tuple-request, with domain error.
  (
    request:
    $req:ty,surface:
    $surf:ident,request_variant:
    $rv:ident(_),response:
    $resp:ty,response_variant:
    $rspv:ident(_),error:
    $err:ty,error_variant:
    $ev:ident(_) $(,)?
  ) => {
    $crate::__impl_bridge_request_body! {
      request: $req,
      surface: $surf,
      request_ctor: $rv,
      request_value: payload,
      request_takes_payload: true,
      response: $resp,
      response_variant: $rspv,
      error_kind: with_error,
      error_type: $err,
      error_variant: $ev,
    }
  };

  // Tuple-request, no domain error.
  (
    request:
    $req:ty,surface:
    $surf:ident,request_variant:
    $rv:ident(_),response:
    $resp:ty,response_variant:
    $rspv:ident(_) $(,)?
  ) => {
    $crate::__impl_bridge_request_body! {
      request: $req,
      surface: $surf,
      request_ctor: $rv,
      request_value: payload,
      request_takes_payload: true,
      response: $resp,
      response_variant: $rspv,
      error_kind: without_error,
    }
  };

  // Unit-request, no domain error.
  (
    request:
    $req:ty,surface:
    $surf:ident,request_variant:
    $rv:ident,response:
    $resp:ty,response_variant:
    $rspv:ident(_) $(,)?
  ) => {
    $crate::__impl_bridge_request_body! {
      request: $req,
      surface: $surf,
      request_ctor: $rv,
      request_value: payload,
      request_takes_payload: false,
      response: $resp,
      response_variant: $rspv,
      error_kind: without_error,
    }
  };
}

#[macro_export]
#[doc(hidden)]
macro_rules! __impl_bridge_request_body {
  (
    request:
    $req:ty,surface:
    $surf:ident,request_ctor:
    $rv:ident,request_value:
    $rval:ident,request_takes_payload: true,response:
    $resp:ty,response_variant:
    $rspv:ident,error_kind: with_error,error_type:
    $err:ty,error_variant:
    $ev:ident $(,)?
  ) => {
    ::paste::paste! {
      impl $crate::gateway::BridgeRequest for $req {
        type Response = $resp;
        type DomainError = $err;

        fn extract(
          data: $crate::gateway::GatewayToBridgeMsgData,
        ) -> ::core::result::Result<Self::Response, $crate::gateway::RequestError<Self::DomainError>> {
          match data {
            $crate::gateway::GatewayToBridgeMsgData::$surf(
              $crate::gateway::[<GatewayToBridge $surf Msg>]::$rspv(v),
            ) => ::core::result::Result::Ok(v),
            $crate::gateway::GatewayToBridgeMsgData::$surf(
              $crate::gateway::[<GatewayToBridge $surf Msg>]::$ev(e),
            ) => ::core::result::Result::Err($crate::gateway::RequestError::Domain(e)),
            $crate::gateway::GatewayToBridgeMsgData::Error(e) => {
              ::core::result::Result::Err($crate::gateway::RequestError::Protocol(e))
            }
            _ => ::core::result::Result::Err($crate::gateway::RequestError::ResponseMismatch),
          }
        }

        fn encode_response(v: Self::Response) -> $crate::gateway::GatewayToBridgeMsgData {
          $crate::gateway::GatewayToBridgeMsgData::$surf(
            $crate::gateway::[<GatewayToBridge $surf Msg>]::$rspv(v),
          )
        }

        fn encode_domain_error(e: Self::DomainError) -> $crate::gateway::GatewayToBridgeMsgData {
          $crate::gateway::GatewayToBridgeMsgData::$surf(
            $crate::gateway::[<GatewayToBridge $surf Msg>]::$ev(e),
          )
        }
      }

      impl ::core::convert::From<$req> for $crate::gateway::BridgeToGatewayMsgData {
        fn from($rval: $req) -> Self {
          $crate::gateway::BridgeToGatewayMsgData::$surf(
            $crate::gateway::[<BridgeToGateway $surf Msg>]::$rv($rval),
          )
        }
      }

      impl $req {
        /// Returns true if `msg` is the typed-response or domain-error
        /// variant of this request. Used by upstream filters to drop
        /// stray response-shape arrivals that bypass the request-id
        /// match (timed-out, late, non-SDK senders).
        pub fn is_response_variant(
          msg: &$crate::gateway::[<GatewayToBridge $surf Msg>],
        ) -> bool {
          ::core::matches!(
            msg,
            $crate::gateway::[<GatewayToBridge $surf Msg>]::$rspv(_)
              | $crate::gateway::[<GatewayToBridge $surf Msg>]::$ev(_),
          )
        }
      }
    }
  };

  (
    request:
    $req:ty,surface:
    $surf:ident,request_ctor:
    $rv:ident,request_value:
    $rval:ident,request_takes_payload: true,response:
    $resp:ty,response_variant:
    $rspv:ident,error_kind: without_error
    $(,)?
  ) => {
    ::paste::paste! {
      impl $crate::gateway::BridgeRequest for $req {
        type Response = $resp;
        type DomainError = ::core::convert::Infallible;

        fn extract(
          data: $crate::gateway::GatewayToBridgeMsgData,
        ) -> ::core::result::Result<Self::Response, $crate::gateway::RequestError<Self::DomainError>> {
          match data {
            $crate::gateway::GatewayToBridgeMsgData::$surf(
              $crate::gateway::[<GatewayToBridge $surf Msg>]::$rspv(v),
            ) => ::core::result::Result::Ok(v),
            $crate::gateway::GatewayToBridgeMsgData::Error(e) => {
              ::core::result::Result::Err($crate::gateway::RequestError::Protocol(e))
            }
            _ => ::core::result::Result::Err($crate::gateway::RequestError::ResponseMismatch),
          }
        }

        fn encode_response(v: Self::Response) -> $crate::gateway::GatewayToBridgeMsgData {
          $crate::gateway::GatewayToBridgeMsgData::$surf(
            $crate::gateway::[<GatewayToBridge $surf Msg>]::$rspv(v),
          )
        }

        fn encode_domain_error(err: Self::DomainError) -> $crate::gateway::GatewayToBridgeMsgData {
          match err {}
        }
      }

      impl ::core::convert::From<$req> for $crate::gateway::BridgeToGatewayMsgData {
        fn from($rval: $req) -> Self {
          $crate::gateway::BridgeToGatewayMsgData::$surf(
            $crate::gateway::[<BridgeToGateway $surf Msg>]::$rv($rval),
          )
        }
      }

      impl $req {
        pub fn is_response_variant(
          msg: &$crate::gateway::[<GatewayToBridge $surf Msg>],
        ) -> bool {
          ::core::matches!(
            msg,
            $crate::gateway::[<GatewayToBridge $surf Msg>]::$rspv(_),
          )
        }
      }
    }
  };

  (
    request:
    $req:ty,surface:
    $surf:ident,request_ctor:
    $rv:ident,request_value:
    $rval:ident,request_takes_payload: false,response:
    $resp:ty,response_variant:
    $rspv:ident,error_kind: without_error
    $(,)?
  ) => {
    ::paste::paste! {
      impl $crate::gateway::BridgeRequest for $req {
        type Response = $resp;
        type DomainError = ::core::convert::Infallible;

        fn extract(
          data: $crate::gateway::GatewayToBridgeMsgData,
        ) -> ::core::result::Result<Self::Response, $crate::gateway::RequestError<Self::DomainError>> {
          match data {
            $crate::gateway::GatewayToBridgeMsgData::$surf(
              $crate::gateway::[<GatewayToBridge $surf Msg>]::$rspv(v),
            ) => ::core::result::Result::Ok(v),
            $crate::gateway::GatewayToBridgeMsgData::Error(e) => {
              ::core::result::Result::Err($crate::gateway::RequestError::Protocol(e))
            }
            _ => ::core::result::Result::Err($crate::gateway::RequestError::ResponseMismatch),
          }
        }

        fn encode_response(v: Self::Response) -> $crate::gateway::GatewayToBridgeMsgData {
          $crate::gateway::GatewayToBridgeMsgData::$surf(
            $crate::gateway::[<GatewayToBridge $surf Msg>]::$rspv(v),
          )
        }

        fn encode_domain_error(err: Self::DomainError) -> $crate::gateway::GatewayToBridgeMsgData {
          match err {}
        }
      }

      impl ::core::convert::From<$req> for $crate::gateway::BridgeToGatewayMsgData {
        fn from(_: $req) -> Self {
          $crate::gateway::BridgeToGatewayMsgData::$surf(
            $crate::gateway::[<BridgeToGateway $surf Msg>]::$rv,
          )
        }
      }

      impl $req {
        pub fn is_response_variant(
          msg: &$crate::gateway::[<GatewayToBridge $surf Msg>],
        ) -> bool {
          ::core::matches!(
            msg,
            $crate::gateway::[<GatewayToBridge $surf Msg>]::$rspv(_),
          )
        }
      }
    }
  };
}
