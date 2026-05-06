use libbridgething::gateway::GatewayToBridgePlayerMsgEvent;

use super::{HandlerResult, MsgHandle};
use crate::player::NowPlayingSource;

pub struct PlayerHandler {
  handle: MsgHandle,
}

impl PlayerHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle_event(self, msg: GatewayToBridgePlayerMsgEvent) -> HandlerResult {
    match msg {
      GatewayToBridgePlayerMsgEvent::Delta(update) => {
        self
          .handle
          .state
          .player
          .apply_now_playing(NowPlayingSource::Companion, update)
          .await?;
      }
      GatewayToBridgePlayerMsgEvent::Snapshot(state) => {
        self.handle.state.player.apply_companion_snapshot(state).await?;
      }
      GatewayToBridgePlayerMsgEvent::QueueChanged(snapshot) => {
        self.handle.state.player.apply_companion_queue(snapshot.items).await?;
      }
    }
    Ok(())
  }
}
