use futures::StreamExt;
use tokio::task::JoinHandle;
use zbus::Connection;

mod media_player1;
mod player;

use media_player1::{MediaPlayer1Proxy, MediaPlayer1Track, PlayerStream};
pub use player::*;

pub type PlayerTx = tokio::sync::mpsc::Sender<PlayerEvent>;
pub type PlayerRx = tokio::sync::mpsc::Receiver<PlayerEvent>;

#[derive(Debug)]
pub struct Player {
  conn: Connection,
  player: MediaPlayer1Proxy<'static>,

  rx: PlayerRx,
  _change_handle: JoinHandle<()>,
}

impl Player {
  pub async fn init(mac: &str) -> DBusResult<Self> {
    let path = format!("/org/bluez/hci0/dev_{}/player0", mac.replace(":", "_"));
    tracing::debug!("attempting to connect to player via dbus at path: {:?}", path);

    let conn = Connection::system().await?;
    let player = MediaPlayer1Proxy::builder(&conn).path(path)?.build().await?;
    tracing::debug!("connection to player via dbus created");

    let (tx, rx) = tokio::sync::mpsc::channel(64);
    let mut changes = PlayerChanges {
      tx,

      status: player.receive_status_changed().await,
      track: player.receive_track_changed().await,
      position: player.receive_position_changed().await,
      shuffle: player.receive_shuffle_changed().await,
      repeat: player.receive_repeat_changed().await,
    };

    Ok(Self {
      conn,
      player,

      rx,
      _change_handle: tokio::spawn(async move { changes.spawn().await }),
    })
  }

  pub async fn recv(&mut self) -> Option<PlayerEvent> {
    self.rx.recv().await
  }
}

pub async fn maybe_recv(player: &mut Option<Player>) -> Option<PlayerEvent> {
  let Some(player) = player else {
    return None;
  };

  let res = player.recv().await;
  tracing::trace!("received message from dbus player: {:?}", &res);
  if res.is_none() {
    tracing::error!("player stream appears to be closed?? this is probably bad.");
  }

  res
}

#[derive(Debug)]
pub enum PlayerEvent {
  Status(PlayerStatus),
  Track(PlayerTrack),
  Position(u32),
  Shuffle(PlayerShuffle),
  Repeat(PlayerRepeat),
}

#[derive(Debug)]
pub struct PlayerChanges {
  tx: PlayerTx,

  status: PlayerStream<String>,
  track: PlayerStream<MediaPlayer1Track>,
  position: PlayerStream<u32>,
  shuffle: PlayerStream<String>,
  repeat: PlayerStream<String>,
}

impl PlayerChanges {
  pub async fn spawn(&mut self) {
    loop {
      let event = self.recv().await;

      match event {
        Ok(event) => {
          if let Err(err) = self.tx.send(event).await {
            tracing::error!("failed to forward message from dbus: {:?}", err);
          };
        }
        Err(err) => tracing::error!("error receiving from dbus: {:?}", err),
      }
    }
  }

  async fn recv(&mut self) -> DBusResult<PlayerEvent> {
    tokio::select! {
      Some(status) = self.status.next() => Ok(PlayerEvent::Status(status.get().await?.try_into()?)),
      Some(track) = self.track.next() => Ok(PlayerEvent::Track(track.get().await?.try_into()?)),
      Some(position) = self.position.next() => Ok(PlayerEvent::Position(position.get().await?)),
      Some(shuffle) = self.shuffle.next() => Ok(PlayerEvent::Shuffle(shuffle.get().await?.try_into()?)),
      Some(repeat) = self.repeat.next() => Ok(PlayerEvent::Repeat(repeat.get().await?.try_into()?)),
    }
  }
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
