use std::net::SocketAddr;

use libbridgething::{
  client::BridgeToClientMsgData,
  wire::{MsgMeta, ResponseMeta, WireError, WireRequest},
};
use uuid::Uuid;

use super::ClientHandler;
use crate::{
  bluetooth::BluetoothMan, net::WSResult, state::State, stock::StockSendMsg, transport::TransportController,
};

#[derive(Debug, Clone)]
pub struct MsgHandle {
  pub state: State,
  pub bluetooth: BluetoothMan,
  pub transport: TransportController,

  pub id: Uuid,
  pub from: SocketAddr,
  pub stock_msg_id: Option<usize>,
}

impl MsgHandle {
  pub fn new(handler: &ClientHandler, id: Uuid, from: SocketAddr, stock_msg_id: Option<usize>) -> Self {
    tracing::trace!("creating connection handle for message id {id} from {from}");

    Self {
      state: handler.state.clone(),
      bluetooth: handler.bluetooth.clone(),
      transport: handler.transport.clone(),

      id,
      from,
      stock_msg_id,
    }
  }

  #[allow(unused)]
  pub async fn send(&self, id: Uuid, data: impl Into<BridgeToClientMsgData>, meta: MsgMeta) -> WSResult<()> {
    self.state.bus.send(id, self.from, data, meta, None).await
  }

  #[allow(unused)]
  pub async fn request(&self, data: impl Into<BridgeToClientMsgData>) -> WSResult<()> {
    self
      .state
      .bus
      .send(Uuid::now_v7(), self.from, data, MsgMeta::Request, None)
      .await
  }

  pub async fn respond(&self, data: impl Into<BridgeToClientMsgData>) -> WSResult<()> {
    self
      .state
      .bus
      .send(
        Uuid::now_v7(),
        self.from,
        data,
        MsgMeta::Response(ResponseMeta { request_id: self.id }),
        self.stock_msg_id,
      )
      .await
  }

  pub async fn respond_to<R: WireRequest<Inbound = BridgeToClientMsgData>>(
    &self,
    response: R::Response,
  ) -> WSResult<()> {
    self.respond(R::encode_response(response)).await
  }

  pub async fn respond_err<R: WireRequest<Inbound = BridgeToClientMsgData>>(
    &self,
    err: R::DomainError,
  ) -> WSResult<()> {
    self.respond(R::encode_domain_error(err)).await
  }

  #[allow(unused)]
  pub async fn send_info(&self, data: impl Into<BridgeToClientMsgData>) -> WSResult<()> {
    self
      .state
      .bus
      .send(Uuid::now_v7(), self.from, data, MsgMeta::Event, None)
      .await
  }

  pub async fn send_stock(&self, data: impl Into<StockSendMsg>) -> WSResult<()> {
    self.state.bus.send_stock(self.from, data).await
  }

  /// Mark a verb as recognized-but-not-yet-built. Logs at the
  /// `bridgething::unimplemented` tracing target and ships a
  /// `WireError::Unimplemented` response on the wire so any awaiting
  /// caller resolves immediately. Greppable as `.unimplemented(` for a
  /// source-level inventory of holes.
  pub async fn unimplemented(&self, verb: &str) -> WSResult<()> {
    tracing::warn!(
      target: "bridgething::unimplemented",
      "({}) {verb}",
      &self.from,
    );
    self.respond(WireError::Unimplemented).await
  }
}
