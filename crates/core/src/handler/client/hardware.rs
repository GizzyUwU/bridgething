use libbridgething::client::{BridgeToClientHardwareMsg, ClientToBridgeHardwareMsg, DisplaySetLevel, DisplaySetMode};

use super::{HandlerResult, MsgHandle};

pub struct HardwareHandler {
  handle: MsgHandle,
}

impl HardwareHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: ClientToBridgeHardwareMsg) -> HandlerResult {
    match msg {
      ClientToBridgeHardwareMsg::DisplaySetMode(DisplaySetMode { mode }) => {
        if let Err(err) = self.handle.state.als.set_mode(mode).await {
          tracing::warn!("({}) hardware.displaySetMode failed: {err}", &self.handle.from);
        }
      }
      ClientToBridgeHardwareMsg::DisplaySetLevel(DisplaySetLevel { level }) => {
        match self.handle.state.als.set_level(level).await {
          Ok(Ok(())) => {}
          Ok(Err(err)) => {
            tracing::debug!("({}) hardware.displaySetLevel rejected: {err:?}", &self.handle.from);
          }
          Err(err) => {
            tracing::warn!("({}) hardware.displaySetLevel failed: {err}", &self.handle.from);
          }
        }
      }
      ClientToBridgeHardwareMsg::StateGet => {
        let reply = self.handle.state.als.snapshot_reply().await;
        self
          .handle
          .respond(BridgeToClientHardwareMsg::StateReply(reply))
          .await?;
      }
    }
    Ok(())
  }
}
