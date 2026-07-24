use libbridgething::wire::{MsgMeta, WireEvent, WireRequest};
use uuid::Uuid;

use crate::{connection::Connection, error::SdkError, protocol::Protocol};

pub struct MsgHandle<P: Protocol> {
  conn: Connection<P>,
  id: Uuid,
  meta: MsgMeta,
}

impl<P: Protocol> Clone for MsgHandle<P> {
  fn clone(&self) -> Self {
    Self {
      conn: self.conn.clone(),
      id: self.id,
      meta: self.meta.clone(),
    }
  }
}

impl<P: Protocol> MsgHandle<P> {
  pub(crate) fn new(conn: Connection<P>, id: Uuid, meta: MsgMeta) -> Self {
    Self { conn, id, meta }
  }

  pub fn id(&self) -> Uuid {
    self.id
  }

  pub fn meta(&self) -> &MsgMeta {
    &self.meta
  }

  pub fn is_request(&self) -> bool {
    matches!(self.meta, MsgMeta::Request)
  }

  pub async fn respond(&self, data: P::OutData) -> Result<(), SdkError> {
    self.conn.respond(self.id, data).await
  }

  pub async fn respond_to<R>(&self, response: R::Response) -> Result<(), SdkError>
  where
    R: WireRequest<Outbound = P::InData, Inbound = P::OutData>,
  {
    self.conn.respond_to::<R>(self.id, response).await
  }

  pub async fn respond_err<R>(&self, err: R::DomainError) -> Result<(), SdkError>
  where
    R: WireRequest<Outbound = P::InData, Inbound = P::OutData>,
  {
    self.conn.respond_err::<R>(self.id, err).await
  }

  pub async fn event<E>(&self, event: E) -> Result<(), SdkError>
  where
    E: WireEvent<P::OutData>,
  {
    self.conn.event(event).await
  }
}
