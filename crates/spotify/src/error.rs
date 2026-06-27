use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum Error {
  #[error("http: {0}")]
  Http(#[from] reqwest::Error),
  #[error("websocket: {0}")]
  Ws(#[from] tokio_tungstenite::tungstenite::Error),
  #[error("protobuf: {0}")]
  Protobuf(#[from] protobuf::Error),
  #[error("json: {0}")]
  Json(#[from] serde_json::Error),
  #[error("url: {0}")]
  Url(#[from] url::ParseError),
  #[error("invalid grant")]
  InvalidGrant,
  #[error("not paired")]
  NotPaired,
  #[error("pairing timed out")]
  PairingTimeout,
  #[error("username not resolved")]
  NoUsername,
  #[error("auth: {0}")]
  Auth(String),
  #[error("status {status} on {what}: {body}")]
  Status { what: String, status: u16, body: String },
  #[error("{0}")]
  Other(String),
}

impl Error {
  pub fn other(msg: impl fmt::Display) -> Self {
    Error::Other(msg.to_string())
  }

  pub(crate) fn status(what: impl Into<String>, status: u16, body: impl Into<String>) -> Self {
    Error::Status {
      what: what.into(),
      status,
      body: body.into(),
    }
  }
}
