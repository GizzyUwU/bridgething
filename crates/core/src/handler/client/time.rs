use libbridgething::client::ClientToBridgeTimeMsgRequest;

use super::{HandlerResult, MsgHandle};

pub struct TimeHandler {
  handle: MsgHandle,
}

impl TimeHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: ClientToBridgeTimeMsgRequest) -> HandlerResult {
    match msg {
      ClientToBridgeTimeMsgRequest::Get => Ok(self.handle.unimplemented("time.get").await?),
    }
  }
}
