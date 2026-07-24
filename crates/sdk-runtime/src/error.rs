use libbridgething::wire::{RequestError, WireError};

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

#[derive(Debug, thiserror::Error)]
pub enum SdkError {
  #[error("connection driver stopped")]
  Disconnected,
  #[error("transport: {0}")]
  Transport(#[from] TransportError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestFailure<E> {
  Domain(E),
  Protocol(WireError),
  ResponseMismatch,
  Timeout,
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
