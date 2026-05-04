use std::sync::Arc;

mod state;

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
    let (state_msg, queue_msg) = {
      let guard = self.state.read().await;
      (guard.to_send_state(), guard.to_send_queue())
    };
    self
      .bus
      .broadcast(state_msg, libbridgething::wire::MsgMeta::Event)
      .await?;
    self
      .bus
      .broadcast(queue_msg, libbridgething::wire::MsgMeta::Event)
      .await?;
    Ok(())
  }

  pub async fn apply_now_playing(
    &self,
    source: NowPlayingSource,
    update: libbridgething::NowPlayingUpdate,
  ) -> PlayerResult<()> {
    let (state_msg, queue_msg) = {
      let mut guard = self.state.write().await;
      guard.apply_now_playing(source, update);
      (guard.to_send_state(), guard.to_send_queue())
    };
    self
      .bus
      .broadcast(state_msg, libbridgething::wire::MsgMeta::Event)
      .await?;
    self
      .bus
      .broadcast(queue_msg, libbridgething::wire::MsgMeta::Event)
      .await?;
    Ok(())
  }

  pub async fn iap2_playback_snapshot(&self) -> libbridgething::PlaybackUpdate {
    self.state.read().await.iap2_playback_snapshot()
  }
}

pub type PlayerResult<T> = Result<T, PlayerError>;
#[derive(Debug, thiserror::Error)]
pub enum PlayerError {
  #[error(transparent)]
  WS(#[from] WSError),
}

crate::impl_broadcast_failure_from!(PlayerError);
