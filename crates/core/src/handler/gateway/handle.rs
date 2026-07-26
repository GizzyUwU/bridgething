use bluer::Address;
use libbridgething::{
  gateway::{BridgeToGatewayMsg, BridgeToGatewayMsgData, GatewayToBridgeMsgData},
  wire::{MsgMeta, ResponseMeta, WireRequest},
};
use uuid::Uuid;

use super::GatewayHandler;
use crate::{
  bluetooth::{BluetoothMan, GatewayType, OutboundGatewayMessage},
  state::State,
  transport::TransportController,
};

#[derive(Debug)]
pub struct MsgHandle {
  pub state: State,
  pub bluetooth: BluetoothMan,
  pub transport: TransportController,

  pub id: Uuid,
  pub meta: MsgMeta,
  pub address: Option<Address>,
  pub protocol: GatewayType,
}

impl MsgHandle {
  pub fn new(
    handler: &GatewayHandler,
    id: Uuid,
    meta: MsgMeta,
    address: Option<Address>,
    protocol: GatewayType,
  ) -> Self {
    tracing::trace!("creating connection handle for message id {id} from {address:?} via {protocol:?}");

    Self {
      state: handler.state.clone(),
      bluetooth: handler.bluetooth.clone(),
      transport: handler.transport.clone(),

      id,
      meta,
      address,
      protocol,
    }
  }

  pub async fn send(&self, id: Uuid, data: impl Into<BridgeToGatewayMsgData>, meta: MsgMeta) {
    self
      .bluetooth
      .gateway_man
      .send_all(OutboundGatewayMessage::new(
        self.address,
        BridgeToGatewayMsg {
          id,
          meta,
          data: data.into(),
        },
      ))
      .await
  }

  pub async fn request(&self, data: impl Into<BridgeToGatewayMsgData>) {
    self.send(Uuid::now_v7(), data, MsgMeta::Request).await
  }

  pub async fn respond(&self, data: impl Into<BridgeToGatewayMsgData>) {
    self
      .send(
        Uuid::now_v7(),
        data,
        MsgMeta::Response(ResponseMeta { request_id: self.id }),
      )
      .await
  }

  pub async fn respond_to<R>(&self, response: R::Response)
  where
    R: WireRequest<Inbound = BridgeToGatewayMsgData, Outbound = GatewayToBridgeMsgData>,
  {
    self.respond(R::encode_response(response)).await
  }

  pub async fn respond_err<R>(&self, err: R::DomainError)
  where
    R: WireRequest<Inbound = BridgeToGatewayMsgData, Outbound = GatewayToBridgeMsgData>,
  {
    self.respond(R::encode_domain_error(err)).await
  }

  pub async fn send_info(&self, data: impl Into<BridgeToGatewayMsgData>) {
    self.send(Uuid::now_v7(), data, MsgMeta::Event).await
  }
}
