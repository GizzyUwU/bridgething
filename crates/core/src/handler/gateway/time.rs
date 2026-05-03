use libbridgething::gateway::GatewayToBridgeTimeMsg;

use super::{HandlerResult, MsgHandle};

pub struct TimeHandler {
  handle: MsgHandle,
}

impl TimeHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: GatewayToBridgeTimeMsg) -> HandlerResult {
    match msg {
      GatewayToBridgeTimeMsg::Snapshot(_) => self.handle.unimplemented("gateway:time.snapshot").await,
    }
    Ok(())
  }
}
