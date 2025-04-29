mod bluetooth;
mod interaction;
mod stock;
mod store;
mod system;
mod voice;

use bluetooth::*;
use interaction::*;
use libbridgething::ForwardMessage;
use stock::*;
use store::*;
use system::*;
use voice::*;

mod handle;
pub use handle::*;

use crate::{
  bluetooth::BluetoothMan,
  handler::HandlerError,
  msg::{ClientMode, RecvMsg, RecvMsgData, stock::StockInterAppSend},
  server::WSError,
  state::State,
};

use super::HandlerResult;

pub struct ClientHandler {
  state: State,
  bluetooth: BluetoothMan,
}

impl ClientHandler {
  pub fn new(state: State, bluetooth: BluetoothMan) -> Self {
    Self { state, bluetooth }
  }

  pub async fn handle(&self, msg: RecvMsg) -> HandlerResult {
    let handle = MsgHandle::new(self, msg.id, msg.from, msg.stock_msg_id);

    match msg.data {
      RecvMsgData::Bluetooth(msg) => {
        tokio::spawn(async move { BluetoothHandler::new(handle).handle(msg).await });
      }
      RecvMsgData::Store(msg) => {
        tokio::spawn(async move { StorageHandler::new(handle).handle(msg).await });
      }
      RecvMsgData::System(msg) => {
        tokio::spawn(async move { SystemHandler::new(handle).handle(msg).await });
      }
      RecvMsgData::Voice(msg) => {
        tokio::spawn(async move { VoiceHandler::new(handle).handle(msg).await });
      }
      RecvMsgData::Interaction(msg) => {
        tokio::spawn(async move { InteractionHandler::new(handle).handle(msg).await });
      }
      RecvMsgData::Forward(msg) => {
        tokio::spawn(async move { TopLevelHandler::new(handle).handle_forward(msg).await });
      }

      // stock compatibility
      RecvMsgData::LegacyStock(msg) => {
        tokio::spawn(async move { LegacyStockHandler::new(handle).handle(msg).await });
      }

      // ignored and unsupported
      RecvMsgData::Hole => {
        tracing::trace!("({}) received blackhole message", &handle.from);

        if let Some(msg_id) = handle.stock_msg_id {
          handle.send_stock(StockInterAppSend::make_ack(Some(msg_id))).await?;
        }
      }
      RecvMsgData::Unsupported(msg) => {
        tracing::trace!("({}) received unsupported message: {:?}", &handle.from, msg);

        if let Some(msg_id) = handle.stock_msg_id {
          handle.send_stock(StockInterAppSend::make_ack(Some(msg_id))).await?;
        }
      }

      // switch to legacy compatibility mode
      RecvMsgData::ChangeMode(mode) => {
        if mode == ClientMode::Stock {
          self.bluetooth.profile_man.handle_connection(false).await?;
        };
      }

      RecvMsgData::ConnectionClosed(code, reason) => {
        tracing::info!(
          "({}) connection closed with code {:?}, reason {}",
          &handle.id,
          code,
          reason
        );
      }
      RecvMsgData::Error(error) => {
        tracing::error!("({}) failed to receive message: {:?}", &handle.from, error);
        return Err(HandlerError::WS(WSError::Websocket(error)));
      }
    }

    Ok(())
  }
}

#[derive(Debug)]
struct TopLevelHandler {
  handle: MsgHandle,
}

impl TopLevelHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle_forward(&mut self, data: ForwardMessage) -> HandlerResult {
    tracing::debug!("({:?}) handling forward message", &self.handle.from);
    self.handle.bluetooth.gateway_man.forward_all(data).await;

    Ok(())
  }
}
