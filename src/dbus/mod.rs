use std::time::Duration;

use futures::StreamExt;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use zbus::Connection;

mod media_player1;
mod player;

use media_player1::{MediaPlayer1Proxy, MediaPlayer1Track, PlayerStream};
pub use player::*;

use crate::{
  msg::{PlayerSend, SendMsgMeta},
  ws::{ConnMan, WSError},
};

pub type PlayerTx = tokio::sync::mpsc::Sender<PlayerEvent>;
pub type PlayerRx = tokio::sync::mpsc::Receiver<PlayerEvent>;

#[derive(Debug)]
pub struct Player {
  player: MediaPlayer1Proxy<'static>,
  pub state: PlayerState,

  rx: PlayerRx,
  cancel_token: CancellationToken,
  _change_handle: JoinHandle<()>,
  _avrcp_handle: JoinHandle<DBusResult<()>>,
}

impl Player {
  pub async fn init(device: bluer::Device) -> DBusResult<Self> {
    let path = format!(
      "/org/bluez/hci0/dev_{}/player0",
      device.address().to_string().replace(":", "_")
    );
    tracing::debug!("attempting to connect to player via dbus at path: {:?}", &path);

    let conn = Connection::system().await?;
    let player = MediaPlayer1Proxy::builder(&conn).path(path.clone())?.build().await?;
    tracing::debug!("connection to player via dbus created");

    let cancel_token = CancellationToken::new();
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    let mut changes = PlayerChanges {
      tx,

      status: player.receive_status_changed().await,
      track: player.receive_track_changed().await,
      position: player.receive_position_changed().await,
      shuffle: player.receive_shuffle_changed().await,
      repeat: player.receive_repeat_changed().await,
    };

    let avrcp_cancel = cancel_token.child_token();
    let change_cancel = cancel_token.child_token();

    Ok(Self {
      player,
      state: PlayerState::default(),

      rx,
      _change_handle: tokio::spawn(async move { changes.spawn(change_cancel).await }),
      _avrcp_handle: tokio::spawn(async move { ensure_avrcp(conn, path, device, avrcp_cancel).await }),
      cancel_token,
    })
  }

  pub async fn recv(&mut self) -> Option<PlayerEvent> {
    self.rx.recv().await
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

  pub async fn shuffle(&self, shuffle: PlayerShuffle) -> DBusResult<()> {
    self.player.set_shuffle((&shuffle).into()).await?;
    Ok(())
  }

  pub async fn repeat(&self, repeat: PlayerRepeat) -> DBusResult<()> {
    self.player.set_repeat((&repeat).into()).await?;
    Ok(())
  }

  pub async fn handle_event(&mut self, conn_man: &mut ConnMan, event: PlayerEvent) -> DBusResult<()> {
    match event {
      PlayerEvent::Status(status) => {
        self.state.status = status;
      }
      PlayerEvent::Track(track) => {
        self.state.track = track;
      }
      PlayerEvent::Position(position) => {
        self.state.position = position;
      }
      PlayerEvent::Shuffle(shuffle) => {
        self.state.shuffle = shuffle;
      }
      PlayerEvent::Repeat(repeat) => {
        self.state.repeat = repeat;
      }
    }

    self.send_state(conn_man).await?;

    Ok(())
  }

  pub async fn send_state(&self, conn_man: &mut ConnMan) -> DBusResult<()> {
    conn_man
      .broadcast(PlayerSend::state_from_player_state(&self.state), SendMsgMeta::Info)
      .await?;
    conn_man
      .broadcast(PlayerSend::queue_from_player_state(&self.state), SendMsgMeta::Info)
      .await?;

    Ok(())
  }
}

impl Drop for Player {
  fn drop(&mut self) {
    tracing::trace!("dropping player, cancelling threads...");
    self.cancel_token.cancel();

    self._change_handle.abort();
    self._avrcp_handle.abort();
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

#[derive(Debug, Clone, Default)]
pub struct PlayerState {
  pub status: PlayerStatus,
  pub track: PlayerTrack,
  pub position: usize,
  pub shuffle: PlayerShuffle,
  pub repeat: PlayerRepeat,
}

#[derive(Debug)]
pub enum PlayerEvent {
  Status(PlayerStatus),
  Track(PlayerTrack),
  Position(usize),
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
  pub async fn spawn(&mut self, cancel_token: CancellationToken) {
    loop {
      tokio::select! {
        event = self.recv() => {
          match event {
            Ok(event) => {
              if let Err(err) = self.tx.send(event).await {
                tracing::error!("failed to forward message from dbus: {:?}", err);
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

  async fn recv(&mut self) -> DBusResult<PlayerEvent> {
    tokio::select! {
      Some(status) = self.status.next() => Ok(PlayerEvent::Status(status.get().await?.try_into()?)),
      Some(track) = self.track.next() => Ok(PlayerEvent::Track(track.get().await?.try_into()?)),
      Some(position) = self.position.next() => Ok(PlayerEvent::Position(position.get().await?.try_into()?)),
      Some(shuffle) = self.shuffle.next() => Ok(PlayerEvent::Shuffle(shuffle.get().await?.try_into()?)),
      Some(repeat) = self.repeat.next() => Ok(PlayerEvent::Repeat(repeat.get().await?.try_into()?)),
    }
  }
}

async fn ensure_avrcp(
  conn: Connection,
  path: String,
  device: bluer::Device,
  cancel_token: CancellationToken,
) -> DBusResult<()> {
  tokio::time::sleep(Duration::from_secs(15)).await;
  let player = MediaPlayer1Proxy::builder(&conn).path(path)?.build().await?;

  loop {
    if cancel_token.is_cancelled() {
      tracing::debug!("avrcp cancel token cancelled, exiting...");
      break;
    }

    // tracing::trace!("ensuring avrcp is connected");
    if let Err(err) = player.status().await {
      tracing::debug!("error received from avrcp, attempting reconnect: {:?}", err);
      if super::bt::connect_avrcp(&device).await {
        tracing::debug!("connected to avrcp, quitting this loop");
        break;
      }
    };

    tokio::time::sleep(Duration::from_secs(5)).await;
  }

  Ok(())
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
  #[error("failed to convert a u32 to a usize. this should not happen. {0}")]
  IntType(#[from] std::num::TryFromIntError),
  #[error("websocket error: {0}")]
  WS(#[from] WSError),
}

impl From<Vec<WSError>> for DBusError {
  fn from(errors: Vec<WSError>) -> Self {
    for error in errors {
      tracing::error!("failed to broadcast message: {:?}", error);
    }

    Self::WS(WSError::BroadcastFailed)
  }
}
