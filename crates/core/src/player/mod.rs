use std::sync::Arc;

mod state;

pub use state::NowPlayingSource;
use state::*;
use tokio::sync::RwLock;

use crate::{
  authority::AuthorityRegistry,
  http::{ClientMan, WSError},
};

#[derive(Debug, Clone)]
pub struct Player {
  state: Arc<RwLock<PlayerState>>,
}

impl Player {
  pub fn new(client_man: ClientMan, authority: AuthorityRegistry) -> Self {
    Self {
      state: Arc::new(RwLock::new(PlayerState::new(client_man, authority))),
    }
  }

  pub async fn send_state(&self) -> PlayerResult<()> {
    self.state.read().await.send_state().await
  }

  pub async fn apply_now_playing(
    &self,
    source: NowPlayingSource,
    update: libbridgething::NowPlayingUpdate,
  ) -> PlayerResult<()> {
    self.state.write().await.apply_now_playing(source, update).await
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
