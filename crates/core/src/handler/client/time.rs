use libbridgething::client::{BridgeToClientTimeMsg, ClientToBridgeTimeMsgRequestDispatch, TimeSnapshot};

use super::{HandlerResult, MsgHandle};

pub struct TimeHandler {
  handle: MsgHandle,
}

impl TimeHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }
}

impl ClientToBridgeTimeMsgRequestDispatch for TimeHandler {
  type Output = HandlerResult;

  async fn get(&self) -> HandlerResult {
    let time = self.handle.state.time.snapshot().await;
    self
      .handle
      .respond(BridgeToClientTimeMsg::Snapshot(TimeSnapshot { time }))
      .await?;
    Ok(())
  }
}
