use libbridgething::client::{CapabilitiesGet, CapabilitiesSnapshot, ClientToBridgeCapabilitiesMsgRequestDispatch};

use super::{HandlerResult, MsgHandle};

pub struct CapabilitiesHandler {
  handle: MsgHandle,
}

impl CapabilitiesHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }
}

impl ClientToBridgeCapabilitiesMsgRequestDispatch for CapabilitiesHandler {
  type Output = HandlerResult;

  async fn get(&self) -> HandlerResult {
    let capabilities = self.handle.state.capabilities.snapshot();
    Ok(
      self
        .handle
        .respond_to::<CapabilitiesGet>(CapabilitiesSnapshot { capabilities })
        .await?,
    )
  }
}
