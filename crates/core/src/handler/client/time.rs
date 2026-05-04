use libbridgething::client::{BridgeToClientTimeMsg, ClientToBridgeTimeMsgRequest, TimeSnapshot};

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
      ClientToBridgeTimeMsgRequest::Get => {
        let time = self.handle.state.time.snapshot().await;
        self
          .handle
          .respond(BridgeToClientTimeMsg::Snapshot(TimeSnapshot { time }))
          .await?;
      }
    }
    Ok(())
  }
}
