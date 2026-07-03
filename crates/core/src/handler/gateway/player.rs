use libbridgething::{
  PlayerState,
  gateway::{GatewayToBridgePlayerMsgCommandDispatch, GatewayToBridgePlayerMsgEventDispatch, QueueSnapshot},
};

use super::{HandlerResult, MsgHandle};

pub struct PlayerHandler {
  handle: MsgHandle,
}

impl PlayerHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }
}

impl GatewayToBridgePlayerMsgEventDispatch for PlayerHandler {
  type Output = HandlerResult;

  async fn snapshot(&self, params: PlayerState) -> HandlerResult {
    self.handle.state.player.apply_companion_snapshot(params).await?;
    Ok(())
  }

  async fn queue_changed(&self, params: QueueSnapshot) -> HandlerResult {
    self.handle.state.player.apply_companion_queue(params).await?;
    Ok(())
  }
}

impl GatewayToBridgePlayerMsgCommandDispatch for PlayerHandler {
  type Output = HandlerResult;

  async fn request_spotify_wake(&self) -> HandlerResult {
    self.handle.bluetooth.iap2.transport.wake_spotify().await;
    Ok(())
  }
}
