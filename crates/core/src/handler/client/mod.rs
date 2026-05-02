mod asset;
mod bluetooth;
mod interaction;
mod stock;
mod store;
mod system;
mod voice;

use asset::*;
use bluetooth::*;
use interaction::*;
use libbridgething::{ForwardMessage, wire::WireError};
use stock::*;
use store::*;
use system::*;
use voice::*;

mod handle;
pub use handle::*;

mod msg;
pub use msg::*;

use super::HandlerResult;
use crate::{
  bluetooth::BluetoothMan, handler::HandlerError, net::WSError, state::State, stock::StockInterAppSend,
  transport::TransportController,
};

/// Run a handler future on a fresh task; if it errors, log it AND surface it
/// to the requesting webapp as a typed protocol-level Error so the caller
/// isn't left wondering why their request never came back. Domain errors
/// (when handlers add them later) take a different path through
/// `MsgHandle::respond_err`. This catches the residual `?` propagation.
fn dispatch<F, Fut>(handle: MsgHandle, work: F)
where
  F: FnOnce(MsgHandle) -> Fut + Send + 'static,
  Fut: std::future::Future<Output = HandlerResult> + Send + 'static,
{
  let err_handle = handle.clone();
  tokio::spawn(async move {
    if let Err(e) = work(handle).await {
      tracing::error!("({:?}) handler failed: {:?}", err_handle.from, e);
      let _ = err_handle
        .respond(WireError::HandlerFailed {
          reason: format!("{e:?}"),
        })
        .await;
    }
  });
}

pub struct ClientHandler {
  state: State,
  bluetooth: BluetoothMan,
  transport: TransportController,
}

impl ClientHandler {
  pub fn new(state: State, bluetooth: BluetoothMan, transport: TransportController) -> Self {
    Self {
      state,
      bluetooth,
      transport,
    }
  }

  pub async fn handle(&self, msg: RecvMsg) -> HandlerResult {
    let handle = MsgHandle::new(self, msg.id, msg.from, msg.stock_msg_id);

    match msg.data {
      RecvMsgData::Asset(msg) => {
        dispatch(handle, move |h| async move { AssetHandler::new(h).handle(msg).await });
      }
      RecvMsgData::Bluetooth(msg) => {
        dispatch(
          handle,
          move |h| async move { BluetoothHandler::new(h).handle(msg).await },
        );
      }
      RecvMsgData::Store(msg) => {
        dispatch(handle, move |h| async move { StorageHandler::new(h).handle(msg).await });
      }
      RecvMsgData::System(msg) => {
        dispatch(handle, move |h| async move { SystemHandler::new(h).handle(msg).await });
      }
      RecvMsgData::Voice(msg) => {
        dispatch(handle, move |h| async move { VoiceHandler::new(h).handle(msg).await });
      }
      RecvMsgData::Interaction(msg) => {
        dispatch(
          handle,
          move |h| async move { InteractionHandler::new(h).handle(msg).await },
        );
      }
      RecvMsgData::Forward(msg) => {
        dispatch(handle, move |h| async move {
          TopLevelHandler::new(h).handle_forward(msg).await
        });
      }

      // stock compatibility
      RecvMsgData::LegacyStock(msg) => {
        dispatch(
          handle,
          move |h| async move { LegacyStockHandler::new(h).handle(msg).await },
        );
      }

      // Response-meta inbound messages are intercepted by `ClientListener::recv`
      // and routed to `ClientManager::complete_pending` before they reach the
      // handler. Anything that arrives here is a bug — the listener leaked one.
      RecvMsgData::Response { request_id, .. } => {
        tracing::error!(
          "({}) Response-meta message {request_id} reached the handler — listener interception is broken",
          &handle.from
        );
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
          self.state.peers.resync_stock_connection().await?;
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
    self.handle.bluetooth.gateway_man.broadcast(data).await;

    Ok(())
  }
}
