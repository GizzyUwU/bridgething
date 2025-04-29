use bluer::Address;
use libbridgething::gateway::{BridgeToGatewayMsg, BridgeToGatewayMsgData, GatewayMsgMeta};
use uuid::Uuid;

use crate::{
  bluetooth::{BluetoothMan, GatewayMessage, GatewayType},
  state::State,
};

use super::GatewayHandler;

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
      .send(Uuid::now_v7(), data, GatewayMsgMeta::Response { request_id: self.id })
      .await
  }

  pub async fn send_info(&self, data: impl Into<BridgeToGatewayMsgData>) {
    self.send(Uuid::now_v7(), data, GatewayMsgMeta::Event).await
  }
}
