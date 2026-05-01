use std::sync::Arc;

mod dbus;
mod state;

use dbus::DBusPlayer;
pub use state::NowPlayingSource;
use state::*;
use tokio::sync::RwLock;

use crate::{
  authority::AuthorityRegistry,
  http::{ClientMan, WSError},
};

#[derive(Debug)]
pub struct Player {
  client_man: ClientMan,
  state: Arc<RwLock<PlayerState>>,

  dbus_player: RwLock<Option<DBusPlayer>>,
}

impl Player {
  pub fn new(client_man: ClientMan, authority: AuthorityRegistry) -> Self {
    Self {
      state: Arc::new(RwLock::new(PlayerState::new(client_man.clone(), authority))),

      dbus_player: RwLock::new(None),
      client_man,
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

  pub async fn next(&self) -> PlayerResult<()> {
    if let Some(player) = &*self.dbus_player.read().await {
      player.next().await?;
    }

    Ok(())
  }

  pub async fn prev(&self) -> PlayerResult<()> {
    if let Some(player) = &*self.dbus_player.read().await {
      player.prev().await?;
    }

    Ok(())
  }

  pub async fn play(&self) -> PlayerResult<()> {
    if let Some(player) = &*self.dbus_player.read().await {
      player.play().await?;
    }

    Ok(())
  }

  pub async fn pause(&self) -> PlayerResult<()> {
    if let Some(player) = &*self.dbus_player.read().await {
      player.pause().await?;
    }

    Ok(())
  }

  pub async fn shuffle(&self, shuffle: bool) -> PlayerResult<()> {
    if let Some(player) = &*self.dbus_player.read().await {
      player.shuffle(shuffle.into()).await?;
    }

    Ok(())
  }

  pub async fn repeat(&self, repeat: bool) -> PlayerResult<()> {
    if let Some(player) = &*self.dbus_player.read().await {
      player.repeat(repeat.into()).await?;
    }

    Ok(())
  }

  pub async fn init_dbus_player(&self, device: bluer::Device) -> PlayerResult<()> {
    let mut state_player = self.dbus_player.write().await;

    match DBusPlayer::init(self.client_man.clone(), self.state.clone(), device).await {
      Ok(player) => {
        player.get_current_state().await?;
        *state_player = Some(player)
      }
      Err(err) => tracing::error!("error connecting to player via dbus: {:?}", err),
    };

    Ok(())
  }
}

pub type PlayerResult<T> = Result<T, PlayerError>;
#[derive(Debug, thiserror::Error)]
pub enum PlayerError {
  #[error(transparent)]
  DBus(#[from] dbus::DBusError),
  #[error(transparent)]
  WS(#[from] WSError),
}

crate::impl_broadcast_failure_from!(PlayerError);
