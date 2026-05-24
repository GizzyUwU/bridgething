use libbridgething::{TimeInfo, gateway::GatewayToBridgeTimeMsgEventDispatch};

use super::{HandlerResult, MsgHandle};

pub struct TimeHandler {
  handle: MsgHandle,
}

impl TimeHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }
}

impl GatewayToBridgeTimeMsgEventDispatch for TimeHandler {
  type Output = HandlerResult;

  async fn snapshot(&self, params: TimeInfo) -> HandlerResult {
    if let Err(err) = self.handle.state.time.apply_companion_snapshot(params).await {
      tracing::warn!(?err, "failed to apply companion time snapshot");
    }
    Ok(())
  }
}
