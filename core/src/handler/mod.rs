pub mod client;
pub mod gateway;

pub use client::ClientHandler;
pub use gateway::GatewayHandler;

use crate::{bluetooth::BluetoothError, player::PlayerError, state::StateError, ws::WSError};

type HandlerResult = Result<(), HandlerError>;

#[derive(Debug, thiserror::Error)]
pub enum HandlerError {
  #[error("websocket communication error: {0}")]
  WS(#[from] WSError),
  #[error("state error: {0}")]
  State(#[from] StateError),
  #[error("bluez error: {0}")]
  Bluez(#[from] bluer::Error),
  #[error("bluetooth handler error: {0}")]
  Bluetooth(#[from] BluetoothError),
  #[error("io error: {0}")]
  IO(#[from] std::io::Error),
  #[error(transparent)]
  Player(#[from] PlayerError),
}
