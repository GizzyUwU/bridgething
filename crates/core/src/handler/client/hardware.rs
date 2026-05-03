use libbridgething::client::ClientToBridgeHardwareMsg;

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
      ClientToBridgeHardwareMsg::DisplaySetMode(_) => Ok(self.handle.unimplemented("hardware.displaySetMode").await?),
      ClientToBridgeHardwareMsg::DisplaySetLevel(_) => Ok(self.handle.unimplemented("hardware.displaySetLevel").await?),
      ClientToBridgeHardwareMsg::StateGet => Ok(self.handle.unimplemented("hardware.stateGet").await?),
    }
  }
}
