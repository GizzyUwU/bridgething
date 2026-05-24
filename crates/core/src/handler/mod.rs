pub mod client;
pub mod gateway;
pub mod iap2;

pub use client::ClientHandler;
pub use gateway::GatewayHandler;
pub use iap2::Iap2EventRouter;

use crate::{
  asset::AssetError,
  bluetooth::BluetoothError,
  impl_broadcast_failure_from,
  net::WSError,
  player::PlayerError,
  state::{AudioError, StateError},
};

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
  #[error(transparent)]
  Asset(#[from] AssetError),
  #[error(transparent)]
  Audio(#[from] AudioError),
}

impl_broadcast_failure_from!(HandlerError);
