use std::net::SocketAddr;

use libbridgething::{ServerEventData, ServerEventType};
use uuid::Uuid;

use crate::{
  msg::stock::StockSendMsg,
  ws::{ClientMan, WSResult},
};

#[derive(Debug)]
pub struct MsgHandle {
  pub client_man: ClientMan,

  pub id: Uuid,
  pub from: SocketAddr,
  pub stock_msg_id: Option<usize>,
}

impl MsgHandle {
  pub fn new(client_man: ClientMan, id: Uuid, from: SocketAddr, stock_msg_id: Option<usize>) -> Self {
    tracing::trace!("creating connection handle for message id {id} from {from}");

    Self {
      client_man,

      id,
      from,
      stock_msg_id,
    }
  }

  pub async fn send(&self, id: Uuid, data: impl Into<ServerEventData>, meta: ServerEventType) -> WSResult<()> {
    self.client_man.send(id, self.from, data, meta, None).await
  }

  pub async fn request(&self, data: impl Into<ServerEventData>) -> WSResult<()> {
    self
      .client_man
      .send(Uuid::now_v7(), self.from, data, ServerEventType::Request, None)
      .await
  }

  pub async fn respond(&self, data: impl Into<ServerEventData>) -> WSResult<()> {
    self
      .client_man
      .send(self.id, self.from, data, ServerEventType::Response, self.stock_msg_id)
      .await
  }

  pub async fn send_info(&self, data: impl Into<ServerEventData>) -> WSResult<()> {
    self
      .client_man
      .send(Uuid::now_v7(), self.from, data, ServerEventType::Info, None)
      .await
  }

  pub async fn send_stock(&self, data: impl Into<StockSendMsg>) -> WSResult<()> {
    self.client_man.send_stock(self.from, data).await
  }
}
