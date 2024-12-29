use futures::StreamExt;
use zbus::Connection;

mod media_player1;
mod player;

use media_player1::{MediaPlayer1Proxy, MediaPlayer1Track, PlayerStream};
pub use player::*;

#[derive(Debug)]
pub struct Player {
  conn: Connection,
  pub player: MediaPlayer1Proxy<'static>,

  status_changes: PlayerStream<String>,
  track_changes: PlayerStream<MediaPlayer1Track>,
  position_changes: PlayerStream<u32>,
  shuffle_changes: PlayerStream<String>,
  repeat_changes: PlayerStream<String>,
}

impl Player {
  pub async fn init(mac: &str) -> DBusResult<Self> {
    let path = format!("/org/bluez/hci0/dev_{}/player0", mac.replace(":", "_"));
    tracing::debug!("attempting to connect to player via dbus at path: {:?}", path);

    let conn = Connection::system().await?;
    let player = MediaPlayer1Proxy::builder(&conn).path(path)?.build().await?;
    tracing::debug!("connection to player via dbus created");

    Ok(Self {
      status_changes: player.receive_status_changed().await,
      track_changes: player.receive_track_changed().await,
      position_changes: player.receive_position_changed().await,
      shuffle_changes: player.receive_shuffle_changed().await,
      repeat_changes: player.receive_repeat_changed().await,

      conn,
      player,
    })
  }

  pub async fn recv(&mut self) -> DBusResult<PlayerEvent> {
    tokio::select! {
      Some(status) = self.status_changes.next() => Ok(PlayerEvent::Status(status.get().await?.try_into()?)),
      Some(track) = self.track_changes.next() => Ok(PlayerEvent::Track(track.get().await?.try_into()?)),
      Some(position) = self.position_changes.next() => Ok(PlayerEvent::Position(position.get().await?)),
      Some(shuffle) = self.shuffle_changes.next() => Ok(PlayerEvent::Shuffle(shuffle.get().await?.try_into()?)),
      Some(repeat) = self.repeat_changes.next() => Ok(PlayerEvent::Repeat(repeat.get().await?.try_into()?)),
    }
  }
}

pub async fn maybe_recv(player: &mut Option<Player>) -> Option<DBusResult<PlayerEvent>> {
  let Some(player) = player else {
    return None;
  };

  let res = player.recv().await;
  tracing::trace!("received message from dbus player: {:?}", &res);

  Some(res)
}

#[derive(Debug)]
pub enum PlayerEvent {
  Status(PlayerStatus),
  Track(PlayerTrack),
  Position(u32),
  Shuffle(PlayerShuffle),
  Repeat(PlayerRepeat),
}

pub type DBusResult<T> = Result<T, DBusError>;
#[derive(Debug, thiserror::Error)]
pub enum DBusError {
  #[error(transparent)]
  DbusError(#[from] zbus::Error),
  #[error(transparent)]
  DbusFdoError(#[from] zbus::fdo::Error),
  #[error(transparent)]
  DbusZvariantError(#[from] zbus::zvariant::Error),
  #[error("failed to deserialize dbus value: {0}")]
  Deserialization(String),
}
