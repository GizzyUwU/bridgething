use bluer::Address;
use libbridgething::gateway::{
  BridgeToGatewayMsg, BridgeToGatewayMsgData, GatewayMsgMeta, GatewayRequest, ResponseMeta,
};
use uuid::Uuid;

use super::GatewayHandler;
use crate::{
  bluetooth::{BluetoothMan, GatewayMessage, GatewayType},
  state::State,
};

#[derive(Debug)]
pub struct MsgHandle {
  pub state: State,
  pub bluetooth: BluetoothMan,

  pub id: Uuid,
  pub meta: GatewayMsgMeta,
  pub address: Option<Address>,
  pub protocol: GatewayType,
}

impl MsgHandle {
  pub fn new(
    handler: &GatewayHandler,
    id: Uuid,
    meta: GatewayMsgMeta,
    address: Option<Address>,
    protocol: GatewayType,
  ) -> Self {
    tracing::trace!(
      "creating connection handle for {:?} message id {} from {:?}",
      protocol,
      id,
      address
    );

    Self {
      state: handler.state.clone(),
      bluetooth: handler.bluetooth.clone(),

      id,
      meta,
      address,
      protocol,
    }
  }

  pub async fn send(&self, id: Uuid, data: impl Into<BridgeToGatewayMsgData>, meta: GatewayMsgMeta) {
    self
      .bluetooth
      .gateway_man
      .send_all(GatewayMessage::new(
        self.address,
        self.protocol,
        BridgeToGatewayMsg {
          id,
          meta,
          data: data.into(),
        },
      ))
      .await
  }

  pub async fn request(&self, data: impl Into<BridgeToGatewayMsgData>) {
    self.send(Uuid::now_v7(), data, GatewayMsgMeta::Request).await
  }

  pub async fn respond(&self, data: impl Into<BridgeToGatewayMsgData>) {
    self
      .send(
        Uuid::now_v7(),
        data,
        GatewayMsgMeta::Response(ResponseMeta { request_id: self.id }),
      )
      .await
  }

  /// Ship a typed success response. The request type fixes the wire variant
  /// so the handler can't accidentally encode the wrong shape.
  pub async fn respond_to<R: GatewayRequest>(&self, response: R::Response) {
    self.respond(R::encode_response(response)).await
  }

  /// Ship a typed domain error response paired with this request type.
  pub async fn respond_err<R: GatewayRequest>(&self, err: R::DomainError) {
    self.respond(R::encode_domain_error(err)).await
  }

  pub async fn send_info(&self, data: impl Into<BridgeToGatewayMsgData>) {
    self.send(Uuid::now_v7(), data, GatewayMsgMeta::Event).await
  }
}
