use std::net::SocketAddr;

use libbridgething::{ServerEventData, ServerEventType, client::ClientRequest};
use uuid::Uuid;

use super::ClientHandler;
use crate::{
  bluetooth::BluetoothMan, http::WSResult, state::State, stock::StockSendMsg, transport::TransportController,
};

// TODO: don't allow cloning of message handle
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
  pub async fn send(&self, id: Uuid, data: impl Into<ServerEventData>, meta: ServerEventType) -> WSResult<()> {
    self.state.client_man.send(id, self.from, data, meta, None).await
  }

  #[allow(unused)]
  pub async fn request(&self, data: impl Into<ServerEventData>) -> WSResult<()> {
    self
      .state
      .client_man
      .send(Uuid::now_v7(), self.from, data, ServerEventType::Request, None)
      .await
  }

  pub async fn respond(&self, data: impl Into<ServerEventData>) -> WSResult<()> {
    self
      .state
      .client_man
      .send(
        Uuid::now_v7(),
        self.from,
        data,
        ServerEventType::Response { request_id: self.id },
        self.stock_msg_id,
      )
      .await
  }

  pub async fn respond_to<R: ClientRequest>(&self, response: R::Response) -> WSResult<()> {
    self.respond(R::encode_response(response)).await
  }

  pub async fn respond_err<R: ClientRequest>(&self, err: R::DomainError) -> WSResult<()> {
    self.respond(R::encode_domain_error(err)).await
  }

  #[allow(unused)]
  pub async fn send_info(&self, data: impl Into<ServerEventData>) -> WSResult<()> {
    self
      .state
      .client_man
      .send(Uuid::now_v7(), self.from, data, ServerEventType::Event, None)
      .await
  }

  pub async fn send_stock(&self, data: impl Into<StockSendMsg>) -> WSResult<()> {
    self.state.client_man.send_stock(self.from, data).await
  }
}
