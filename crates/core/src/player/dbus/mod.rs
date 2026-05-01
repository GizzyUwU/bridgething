use std::sync::Arc;

use futures::StreamExt;
use tokio::{sync::RwLock, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use zbus::Connection;

mod media_player1;
mod state;

use media_player1::{DBusPlayerStream, MediaPlayer1Proxy, MediaPlayer1Track};
pub use state::*;

use super::state::PlayerState;
use crate::http::{ClientMan, WSError};

#[derive(Debug)]
pub struct DBusPlayer {
  state: Arc<RwLock<PlayerState>>,
  player: MediaPlayer1Proxy<'static>,

  cancel_token: CancellationToken,
  _change_handle: JoinHandle<()>,
}

impl DBusPlayer {
  pub async fn init(
    _client_man: ClientMan,
    state: Arc<RwLock<PlayerState>>,
    device: bluer::Device,
  ) -> DBusResult<Self> {
    let path = format!(
      "/org/bluez/hci0/dev_{}/player0",
      device.address().to_string().replace(":", "_")
    );
    tracing::debug!("attempting to connect to player via dbus at path: {:?}", &path);

    let conn = Connection::system().await?;
    let player = MediaPlayer1Proxy::builder(&conn).path(path.clone())?.build().await?;
    tracing::debug!("connection to player via dbus created");

    let cancel_token = CancellationToken::new();
    let changes = DBusPlayerChanges::init(state.clone(), &player).await;
    let change_cancel = cancel_token.child_token();

    Ok(Self {
      state,
      player,

      _change_handle: tokio::spawn(async move { changes.spawn(change_cancel).await }),
      cancel_token,
    })
  }

  pub async fn next(&self) -> DBusResult<()> {
    Ok(self.player.next().await?)
  }

  pub async fn prev(&self) -> DBusResult<()> {
    Ok(self.player.previous().await?)
  }

  pub async fn play(&self) -> DBusResult<()> {
    Ok(self.player.play().await?)
  }

  pub async fn pause(&self) -> DBusResult<()> {
    Ok(self.player.pause().await?)
  }

  pub async fn shuffle(&self, shuffle: DBusPlayerShuffle) -> DBusResult<()> {
    self.player.set_shuffle((&shuffle).into()).await?;
    Ok(())
  }

  pub async fn repeat(&self, repeat: DBusPlayerRepeat) -> DBusResult<()> {
    self.player.set_repeat((&repeat).into()).await?;
    Ok(())
  }

  // TODO: anything but this
  pub async fn get_current_state(&self) -> DBusResult<()> {
    let mut state = self.state.write().await;

    if let Err(err) = state
      .handle_dbus_event(DBusPlayerEvent::Status(self.player.status().await?.try_into()?))
      .await
    {
      tracing::error!("failed to handle message from dbus: {:?}", err);
    }

    if let Err(err) = state
      .handle_dbus_event(DBusPlayerEvent::Track(self.player.track().await?.try_into()?))
      .await
    {
      tracing::error!("failed to handle message from dbus: {:?}", err);
    }

    if let Err(err) = state
      .handle_dbus_event(DBusPlayerEvent::Position(self.player.position().await?.try_into()?))
      .await
    {
      tracing::error!("failed to handle message from dbus: {:?}", err);
    }

    if let Err(err) = state
      .handle_dbus_event(DBusPlayerEvent::Shuffle(self.player.shuffle().await?.try_into()?))
      .await
    {
      tracing::error!("failed to handle message from dbus: {:?}", err);
    }

    if let Err(err) = state
      .handle_dbus_event(DBusPlayerEvent::Repeat(self.player.repeat().await?.try_into()?))
      .await
    {
      tracing::error!("failed to handle message from dbus: {:?}", err);
    }

    Ok(())
  }
}

impl Drop for DBusPlayer {
  fn drop(&mut self) {
    tracing::trace!("dropping player, cancelling threads...");
    self.cancel_token.cancel();
  }
}

#[derive(Debug)]
pub enum DBusPlayerEvent {
  Status(DBusPlayerStatus),
  Track(DBusPlayerTrack),
  Position(usize),
  Shuffle(DBusPlayerShuffle),
  Repeat(DBusPlayerRepeat),
}

#[derive(Debug)]
pub struct DBusPlayerChanges {
  state: Arc<RwLock<PlayerState>>,

  status: DBusPlayerStream<String>,
  track: DBusPlayerStream<MediaPlayer1Track>,
  position: DBusPlayerStream<u32>,
  shuffle: DBusPlayerStream<String>,
  repeat: DBusPlayerStream<String>,
}

impl DBusPlayerChanges {
  pub async fn init(state: Arc<RwLock<PlayerState>>, player: &MediaPlayer1Proxy<'static>) -> Self {
    Self {
      state,

      status: player.receive_status_changed().await,
      track: player.receive_track_changed().await,
      position: player.receive_position_changed().await,
      shuffle: player.receive_shuffle_changed().await,
      repeat: player.receive_repeat_changed().await,
    }
  }

  pub async fn spawn(mut self, cancel_token: CancellationToken) {
    loop {
      tokio::select! {
        event = self.recv() => {
          match event {
            Ok(event) => {
              if let Err(err) = self.state.write().await.handle_dbus_event(event).await {
                tracing::error!("failed to handle message from dbus: {:?}", err);
              };
            }
            Err(err) => tracing::error!("error receiving from dbus: {:?}", err),
          }
        }
        _ = cancel_token.cancelled() => {
          tracing::debug!("avrcp cancel token cancelled, exiting...");
          break;
        }
      }
    }
  }

  async fn recv(&mut self) -> DBusResult<DBusPlayerEvent> {
    tokio::select! {
      Some(status) = self.status.next() => Ok(DBusPlayerEvent::Status(status.get().await?.try_into()?)),
      Some(track) = self.track.next() => Ok(DBusPlayerEvent::Track(track.get().await?.try_into()?)),
      Some(position) = self.position.next() => Ok(DBusPlayerEvent::Position(position.get().await?.try_into()?)),
      Some(shuffle) = self.shuffle.next() => Ok(DBusPlayerEvent::Shuffle(shuffle.get().await?.try_into()?)),
      Some(repeat) = self.repeat.next() => Ok(DBusPlayerEvent::Repeat(repeat.get().await?.try_into()?)),
    }
  }
}

pub type DBusResult<T> = Result<T, DBusError>;
#[derive(Debug, thiserror::Error)]
#[allow(clippy::large_enum_variant)]
pub enum DBusError {
  #[error(transparent)]
  DbusError(#[from] zbus::Error),
  #[error(transparent)]
  DbusFdoError(#[from] zbus::fdo::Error),
  #[error(transparent)]
  DbusZvariantError(#[from] zbus::zvariant::Error),
  #[error("failed to deserialize dbus value: {0}")]
  Deserialization(String),
  #[error("failed to convert a u32 to a usize. this should not happen. {0}")]
  IntType(#[from] std::num::TryFromIntError),
  #[error("websocket error: {0}")]
  WS(#[from] WSError),
}

crate::impl_broadcast_failure_from!(DBusError);
