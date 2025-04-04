// TODO: gateway version of MsgHandle
// mod handle;
// pub use handle::*;

use libbridgething::gateway::GatewayToBridgeMsg;

use crate::{
  bluetooth::{BluetoothMan, GatewayType},
  state::State,
};

use super::HandlerResult;

pub struct GatewayHandler {
  state: State,
  bluetooth: BluetoothMan,
}

impl GatewayHandler {
  pub fn new(state: State, bluetooth: BluetoothMan) -> Self {
    Self { state, bluetooth }
  }

  pub async fn handle(&self, from: GatewayType, event: GatewayToBridgeMsg) -> HandlerResult {
    tracing::trace!("handling {from:?} bluetooth event: {event:?}");

    // TODO: anything else lmao
    // let handle = MsgHandle::new(self, msg.id, msg.from, msg.stock_msg_id);

    Ok(())
  }
}
