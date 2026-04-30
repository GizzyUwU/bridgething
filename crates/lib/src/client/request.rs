//! Typed request/response pairing for the local websocket protocol.
//!
//! Mirror of `gateway::GatewayRequest` for the client (webapp) → bridge
//! direction. A `ClientRequest` ties a request type to its response type and
//! optional domain-error type, so webapp callers get `client.call(req).await
//! -> Response` and bridge handlers can `respond_to::<Req>(response)` without
//! naming the wire variant by hand.
//!
//! Implementations are produced by the `impl_client_request!` macro (see
//! crate root).

use crate::client::ClientCommandType;
use crate::gateway::RequestError;
use crate::server::ServerEventData;

/// client (webapp) → bridge: a typed request whose response shape is statically known.
pub trait ClientRequest: Into<ClientCommandType> {
  type Response;
  type DomainError;

  fn extract(data: ServerEventData) -> Result<Self::Response, RequestError<Self::DomainError>>;
  fn encode_response(response: Self::Response) -> ServerEventData;
  fn encode_domain_error(err: Self::DomainError) -> ServerEventData;
}

/// Generate `ClientRequest` impl + `From<Req> for ClientCommandType`. Same
/// shape as `impl_gateway_request!`: with-domain-error (six named sections)
/// and without (three named sections; sets `DomainError = Infallible`).
///
/// ```ignore
/// impl_client_request! {
///   request: KVGet,
///   response: StorageResponse,
///   encode_request:
///     r => ClientCommandType::Store(ClientKVStoreCommand::Get { key: r.key }),
///   extract_response:
///     ServerEventData::Storage(ServerStorageEvent::Response(v)) => v,
///   encode_response:
///     v => ServerEventData::Storage(ServerStorageEvent::Response(v)),
/// }
/// ```
#[macro_export]
macro_rules! impl_client_request {
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
    impl $crate::client::ClientRequest for $req {
      type Response = $resp;
      type DomainError = $err;

      fn extract(
        data: $crate::server::ServerEventData,
      ) -> ::core::result::Result<Self::Response, $crate::gateway::RequestError<Self::DomainError>> {
        match data {
          $resp_pat => ::core::result::Result::Ok($resp_extract),
          $err_pat => ::core::result::Result::Err($crate::gateway::RequestError::Domain($err_extract)),
          $crate::server::ServerEventData::Error(e) => {
            ::core::result::Result::Err($crate::gateway::RequestError::Protocol(e))
          }
          _ => ::core::result::Result::Err($crate::gateway::RequestError::ResponseMismatch),
        }
      }

      fn encode_response($resp_id: Self::Response) -> $crate::server::ServerEventData {
        $resp_wrap
      }

      fn encode_domain_error($err_id: Self::DomainError) -> $crate::server::ServerEventData {
        $err_wrap
      }
    }

    impl ::core::convert::From<$req> for $crate::client::ClientCommandType {
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
    impl $crate::client::ClientRequest for $req {
      type Response = $resp;
      type DomainError = ::core::convert::Infallible;

      fn extract(
        data: $crate::server::ServerEventData,
      ) -> ::core::result::Result<Self::Response, $crate::gateway::RequestError<Self::DomainError>> {
        match data {
          $resp_pat => ::core::result::Result::Ok($resp_extract),
          $crate::server::ServerEventData::Error(e) => {
            ::core::result::Result::Err($crate::gateway::RequestError::Protocol(e))
          }
          _ => ::core::result::Result::Err($crate::gateway::RequestError::ResponseMismatch),
        }
      }

      fn encode_response($resp_id: Self::Response) -> $crate::server::ServerEventData {
        $resp_wrap
      }

      fn encode_domain_error(err: Self::DomainError) -> $crate::server::ServerEventData {
        match err {}
      }
    }

    impl ::core::convert::From<$req> for $crate::client::ClientCommandType {
      fn from($req_id: $req) -> Self {
        $req_wrap
      }
    }
  };
}
