use std::sync::Arc;

mod state;

use libbridgething::{
  RepeatMode,
  client::{BridgeToClientPlayerMsg, PlayerQueueReply, PlayerStateReply},
};
pub use state::NowPlayingSource;
use state::*;
use tokio::sync::RwLock;

use crate::{
  authority::AuthorityRegistry,
  net::{WSError, WireEventBus},
};

#[derive(Debug, Clone)]
pub struct Player {
  state: Arc<RwLock<PlayerState>>,
  bus: WireEventBus,
}

impl Player {
  pub fn new(bus: WireEventBus, authority: AuthorityRegistry) -> Self {
    Self {
      state: Arc::new(RwLock::new(PlayerState::new(authority))),
      bus,
    }
  }

  pub async fn send_state(&self) -> PlayerResult<()> {
    let (state_reply, queue_reply) = self.state.read().await.replies();
    self.broadcast_snapshot(state_reply, queue_reply).await
  }

  pub async fn apply_now_playing(
    &self,
    source: NowPlayingSource,
    update: libbridgething::NowPlayingUpdate,
  ) -> PlayerResult<()> {
    {
      let mut guard = self.state.write().await;
      guard.apply_now_playing(source, update);
    }
    let (state_reply, queue_reply) = self.state.read().await.replies();
    self.broadcast_snapshot(state_reply, queue_reply).await
  }

  pub async fn apply_artwork_id(&self, source: NowPlayingSource, asset_id: String) -> PlayerResult<()> {
    {
      let mut guard = self.state.write().await;
      guard.apply_artwork_id(source, asset_id);
    }
    let (state_reply, queue_reply) = self.state.read().await.replies();
    self.broadcast_snapshot(state_reply, queue_reply).await
  }

  pub async fn iap2_shuffle(&self) -> Option<bool> {
    self.state.read().await.iap2_shuffle()
  }

  pub async fn iap2_repeat_mode(&self) -> Option<RepeatMode> {
    self.state.read().await.iap2_repeat_mode()
  }

  pub async fn iap2_set_elapsed_time_available(&self) -> Option<bool> {
    self.state.read().await.iap2_set_elapsed_time_available()
  }

  pub async fn apply_iap2_queue(&self, items: Vec<libbridgething::QueueItem>) -> PlayerResult<()> {
    {
      let mut guard = self.state.write().await;
      guard.replace_iap2_queue(items);
    }
    let queue_reply = self.state.read().await.queue_reply();
    self
      .bus
      .broadcast(
        BridgeToClientPlayerMsg::QueueChanged(queue_reply),
        libbridgething::wire::MsgMeta::Event,
      )
      .await?;
    Ok(())
  }

  pub async fn apply_companion_queue(&self, items: Vec<libbridgething::QueueItem>) -> PlayerResult<()> {
    {
      let mut guard = self.state.write().await;
      guard.replace_companion_queue(items);
    }
    let queue_reply = self.state.read().await.queue_reply();
    self
      .bus
      .broadcast(
        BridgeToClientPlayerMsg::QueueChanged(queue_reply),
        libbridgething::wire::MsgMeta::Event,
      )
      .await?;
    Ok(())
  }

  pub async fn apply_companion_snapshot(&self, snapshot: libbridgething::PlayerState) -> PlayerResult<()> {
    {
      let mut guard = self.state.write().await;
      guard.apply_companion_snapshot(snapshot);
    }
    let (state_reply, queue_reply) = self.state.read().await.replies();
    self.broadcast_snapshot(state_reply, queue_reply).await
  }

  pub async fn state_reply(&self) -> PlayerStateReply {
    self.state.read().await.state_reply()
  }

  pub async fn current_artwork_id(&self) -> Option<String> {
    self.state.read().await.current_artwork_id()
  }

  pub async fn queue_reply(&self) -> PlayerQueueReply {
    self.state.read().await.queue_reply()
  }

  pub async fn apply_transport_intent(&self, playing: bool) -> PlayerResult<()> {
    {
      let mut guard = self.state.write().await;
      guard.set_transport_intent(playing);
    }
    let (state_reply, queue_reply) = self.state.read().await.replies();
    self.broadcast_snapshot(state_reply, queue_reply).await
  }

  pub async fn apply_seek_intent(&self, position_ms: u32) -> PlayerResult<()> {
    {
      let mut guard = self.state.write().await;
      guard.set_seek_intent(position_ms);
    }
    let (state_reply, queue_reply) = self.state.read().await.replies();
    self.broadcast_snapshot(state_reply, queue_reply).await
  }

  async fn broadcast_snapshot(&self, state: PlayerStateReply, queue: PlayerQueueReply) -> PlayerResult<()> {
    self
      .bus
      .broadcast(
        BridgeToClientPlayerMsg::Snapshot(state),
        libbridgething::wire::MsgMeta::Event,
      )
      .await?;
    self
      .bus
      .broadcast(
        BridgeToClientPlayerMsg::QueueChanged(queue),
        libbridgething::wire::MsgMeta::Event,
      )
      .await?;
    Ok(())
  }
}

pub type PlayerResult<T> = Result<T, PlayerError>;
#[derive(Debug, thiserror::Error)]
pub enum PlayerError {
  #[error(transparent)]
  WS(#[from] WSError),
}

crate::impl_broadcast_failure_from!(PlayerError);
