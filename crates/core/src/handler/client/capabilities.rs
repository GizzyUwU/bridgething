use libbridgething::client::ClientToBridgeCapabilitiesMsgRequest;

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
      ClientToBridgeCapabilitiesMsgRequest::Get => Ok(self.handle.unimplemented("capabilities.get").await?),
    }
  }
}
