use std::net::SocketAddr;

use uuid::Uuid;

use crate::{
  msg::{SendMsgData, SendMsgMeta},
  ws::{ConnMan, WSResult},
};

#[derive(Debug)]
pub struct MsgHandle<'a> {
  conn_man: &'a mut ConnMan,

  pub id: Uuid,
  pub from: SocketAddr,
}

impl<'a> MsgHandle<'a> {
  pub fn new(conn_man: &'a mut ConnMan, id: Uuid, from: SocketAddr) -> Self {
    tracing::trace!("creating connection handle for message id {id} from {from}");

    Self { conn_man, id, from }
  }

  pub async fn send(&self, id: Uuid, data: impl Into<SendMsgData>, meta: SendMsgMeta) -> WSResult<()> {
    self.conn_man.send(id, self.from, data, meta).await
  }

  pub async fn request(&self, data: impl Into<SendMsgData>) -> WSResult<()> {
    self
      .conn_man
      .send(Uuid::now_v7(), self.from, data, SendMsgMeta::Request)
      .await
  }

  pub async fn respond(&self, data: impl Into<SendMsgData>) -> WSResult<()> {
    self
      .conn_man
      .send(self.id, self.from, data, SendMsgMeta::Response)
      .await
  }

  pub async fn send_info(&self, data: impl Into<SendMsgData>) -> WSResult<()> {
    self
      .conn_man
      .send(Uuid::now_v7(), self.from, data, SendMsgMeta::Info)
      .await
  }
}
