use std::net::SocketAddr;

mod bluetooth;
mod interaction;
mod storage;
mod system;
mod voice;

use bluetooth::*;
use interaction::*;
use storage::*;
use system::*;
use uuid::Uuid;
use voice::*;

mod handle;

pub use handle::*;

use crate::{
  bt::Bluetooth,
  msg::RecvMsgData,
  state::{State, StateError},
  ws::{ConnMan, WSError},
};

pub struct Handler<'a> {
  handle: MsgHandle<'a>,
  state: &'a mut State,
  bluetooth: &'a mut Bluetooth,
}

impl<'a> Handler<'a> {
  pub fn new(
    conn_man: &'a mut ConnMan,
    state: &'a mut State,
    bluetooth: &'a mut Bluetooth,
    msg_id: Uuid,
    msg_from: SocketAddr,
  ) -> Self {
    Self {
      handle: MsgHandle::new(conn_man, msg_id, msg_from),
      state,
      bluetooth,
    }
  }

  pub async fn handle(self, data: RecvMsgData) -> HandlerResult {
    match data {
      RecvMsgData::Bluetooth(msg) => BluetoothHandler::new(self).handle(msg).await,
      RecvMsgData::Storage(msg) => StorageHandler::new(self).handle(msg).await,
      RecvMsgData::System(msg) => SystemHandler::new(self).handle(msg).await,
      RecvMsgData::Voice(msg) => VoiceHandler::new(self).handle(msg).await,
      RecvMsgData::Interaction { msg, stock_msg_id } => InteractionHandler::new(self, stock_msg_id).handle(msg).await,
      RecvMsgData::Hole => {
        tracing::trace!("({}) received blackhole message, ignoring", &self.handle.id);
        Ok(())
      }
      RecvMsgData::ConnectionClosed(code, reason) => {
        tracing::info!(
          "({}) connection closed with code {:?}, reason {}",
          &self.handle.id,
          code,
          reason
        );
        Ok(())
      }
      RecvMsgData::Error(error) => {
        tracing::error!("({}) failed to receive message: {:?}", &self.handle.id, error);
        Err(HandlerError::WS(WSError::Websocket(error)))
      }
    }
  }
}

type HandlerResult = Result<(), HandlerError>;

#[derive(Debug, thiserror::Error)]
pub enum HandlerError {
  #[error("websocket communication error: {0}")]
  WS(#[from] WSError),
  #[error("state error: {0}")]
  State(#[from] StateError),
  #[error("bluetooth error: {0}")]
  Bluetooth(#[from] bluer::Error),
}
