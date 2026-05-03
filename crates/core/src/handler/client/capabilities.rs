use libbridgething::client::{CapabilitiesGet, CapabilitiesSnapshot, ClientToBridgeCapabilitiesMsgRequest};

use super::{HandlerResult, MsgHandle};

pub struct CapabilitiesHandler {
  handle: MsgHandle,
}

impl CapabilitiesHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: ClientToBridgeCapabilitiesMsgRequest) -> HandlerResult {
    match msg {
      ClientToBridgeCapabilitiesMsgRequest::Get => {
        let capabilities = self.handle.state.capabilities.snapshot();
        Ok(
          self
            .handle
            .respond_to::<CapabilitiesGet>(CapabilitiesSnapshot { capabilities })
            .await?,
        )
      }
    }
  }
}
