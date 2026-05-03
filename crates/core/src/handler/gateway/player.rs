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
        Ok(())
      }
      GatewayToBridgePlayerMsgEvent::Snapshot(_) => {
        tracing::trace!(target: "bridgething::gateway::player", "player snapshot ignored — companion-side authoritative snapshot not yet wired");
        Ok(())
      }
      GatewayToBridgePlayerMsgEvent::QueueChanged(_) => {
        tracing::trace!(target: "bridgething::gateway::player", "queue change ignored — queue mirror not yet wired");
        Ok(())
      }
    }
  }
}
