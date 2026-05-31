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
    self.handle.state.player.apply_companion_queue(params.items).await?;
    Ok(())
  }

  async fn enrichment_offer(&self, params: NowPlayingEnrichment) -> HandlerResult {
    let prefetch = enrichment_prefetch_ids(&params);
    self.handle.state.player.apply_enrichment(params).await?;
    if !prefetch.is_empty() {
      crate::handler::client::preload_assets(self.handle.state.clone(), self.handle.bluetooth.clone(), prefetch).await;
    }
    Ok(())
  }
}

const ENRICHMENT_PREFETCH_QUEUE_DEPTH: usize = 3;

fn enrichment_prefetch_ids(offer: &NowPlayingEnrichment) -> Vec<String> {
  offer
    .head
    .iter()
    .chain(offer.queue.iter().take(ENRICHMENT_PREFETCH_QUEUE_DEPTH))
    .filter_map(|item| item.artwork_id.clone())
    .collect()
}
