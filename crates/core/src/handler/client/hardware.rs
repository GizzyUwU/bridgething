use libbridgething::client::{
  BridgeToClientHardwareMsg, ClientToBridgeHardwareMsgDispatch, DisplaySetLevel, DisplaySetMode,
};

use super::{HandlerResult, MsgHandle};

pub struct HardwareHandler {
  handle: MsgHandle,
}

impl HardwareHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }
}

impl ClientToBridgeHardwareMsgDispatch for HardwareHandler {
  type Output = HandlerResult;

  async fn display_set_mode(&self, params: DisplaySetMode) -> HandlerResult {
    if let Err(err) = self.handle.state.als.set_mode(params.mode).await {
      tracing::warn!("({}) hardware.displaySetMode failed: {err}", &self.handle.from);
    }
    Ok(())
  }

  async fn display_set_level(&self, params: DisplaySetLevel) -> HandlerResult {
    match self.handle.state.als.set_level(params.level).await {
      Ok(Ok(())) => {}
      Ok(Err(err)) => {
        tracing::debug!("({}) hardware.displaySetLevel rejected: {err:?}", &self.handle.from);
      }
      Err(err) => {
        tracing::warn!("({}) hardware.displaySetLevel failed: {err}", &self.handle.from);
      }
    }
    Ok(())
  }

  async fn state_get(&self) -> HandlerResult {
    let reply = self.handle.state.als.snapshot_reply().await;
    self
      .handle
      .respond(BridgeToClientHardwareMsg::StateReply(reply))
      .await?;
    Ok(())
  }
}
