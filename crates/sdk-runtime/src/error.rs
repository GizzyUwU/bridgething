use libbridgething::wire::{RequestError, WireError};

/// transport-layer failure.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
  #[error("transport closed")]
  Closed,
  #[error("encode failed: {0}")]
  Encode(String),
  #[error("decode failed: {0}")]
  Decode(String),
  #[error("io: {0}")]
  Io(#[from] std::io::Error),
}

/// failure of a fire-and-forget send (`command` / `event` / `send_data`).
#[derive(Debug, thiserror::Error)]
pub enum SdkError {
  #[error("connection driver stopped")]
  Disconnected,
  #[error("transport: {0}")]
  Transport(#[from] TransportError),
}

/// flattened failure of a typed `request`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestFailure<E> {
  /// the request's own error catalog.
  Domain(E),
  /// protocol-level failure reported by the responder.
  Protocol(WireError),
  /// the response wire shape didn't match what the request declared.
  ResponseMismatch,
  /// no response within the connection's request timeout.
  Timeout,
  /// the connection driver stopped before a response arrived.
  Disconnected,
}

impl<E> From<RequestError<E>> for RequestFailure<E> {
  fn from(err: RequestError<E>) -> Self {
    match err {
      RequestError::Domain(d) => Self::Domain(d),
      RequestError::Protocol(w) => Self::Protocol(w),
      RequestError::ResponseMismatch => Self::ResponseMismatch,
    }
  }
}
