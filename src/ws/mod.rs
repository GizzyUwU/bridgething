mod connection;
mod connman;
mod server;

pub use connman::ConnMan;
pub use server::Server;

use crate::msg::SendMsg;

type WSResult<T> = Result<T, WSError>;

#[derive(Debug, thiserror::Error)]
pub enum WSError {
  #[error("failed to bind to port: {0}")]
  Bind(#[from] std::io::Error),
  #[error("websocket error: {0}")]
  Websocket(#[from] tokio_websockets::Error),
  #[error("requested client to send to is not connected to the server!!")]
  NotConnected,
  #[error("could not send a message to requested client: {0}")]
  MessageSend(#[from] tokio::sync::mpsc::error::SendError<SendMsg>),
  #[error("channel from connections to server struct has been dropped!!! this is bad.")]
  ChannelClosed,
}
