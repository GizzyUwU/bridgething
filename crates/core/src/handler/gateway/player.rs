use libbridgething::{
  NowPlayingUpdate, PlayerState,
  gateway::{GatewayToBridgePlayerMsgEventDispatch, NowPlayingEnrichment, QueueSnapshot},
};

use super::{HandlerResult, MsgHandle};
use crate::player::NowPlayingSource;

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

  async fn delta(&self, params: NowPlayingUpdate) -> HandlerResult {
    self
      .handle
      .state
      .player
      .apply_now_playing(NowPlayingSource::Companion, params)
      .await?;
    Ok(())
  }

  async fn queue_changed(&self, params: QueueSnapshot) -> HandlerResult {
    self.handle.state.player.apply_companion_queue(params).await?;
    Ok(())
  }

  async fn enrichment_offer(&self, params: NowPlayingEnrichment) -> HandlerResult {
    self.handle.state.player.apply_enrichment(params).await?;
    Ok(())
  }
}
