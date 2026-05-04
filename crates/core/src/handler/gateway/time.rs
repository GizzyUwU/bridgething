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
      GatewayToBridgeTimeMsg::Snapshot(info) => {
        if let Err(err) = self.handle.state.time.apply_companion_snapshot(info).await {
          tracing::warn!(?err, "failed to apply companion time snapshot");
        }
      }
    }
    Ok(())
  }
}
