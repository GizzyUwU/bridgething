use std::time::Duration;

use futures::StreamExt;
use libbridgething::{server::ServerPlayerEvent, DeviceType, PlaybackOptions, PlaybackRestrictions, ServerEventType};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use zbus::Connection;

mod media_player1;
mod player;

use media_player1::{DBusPlayerStream, MediaPlayer1Proxy, MediaPlayer1Track};
pub use player::*;

use crate::{
  bt::art::CoverArt,
  handler::MsgHandle,
  state::art::CoverArtCache,
  ws::{ClientMan, WSError},
};

pub type PlayerTx = tokio::sync::mpsc::Sender<DBusPlayerEvent>;
pub type PlayerRx = tokio::sync::mpsc::Receiver<DBusPlayerEvent>;

#[derive(Debug)]
pub struct Player {
  client_man: ClientMan,
  player: MediaPlayer1Proxy<'static>,
  pub state: PlayerState,
  art: Option<CoverArt>,

  rx: PlayerRx,
  cancel_token: CancellationToken,
  _change_handle: JoinHandle<()>,
  _avrcp_handle: JoinHandle<DBusResult<()>>,
}

impl Player {
  pub async fn init(client_man: ClientMan, cover_art_cache: CoverArtCache, device: bluer::Device) -> DBusResult<Self> {
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
    let changes = DBusPlayerChanges::init(tx, &player).await;

    let avrcp_cancel = cancel_token.child_token();
    let change_cancel = cancel_token.child_token();

    let mut state = PlayerState::default();

    let mut art = None;

    if let Ok(obex_port) = player.obex_port().await {
      if obex_port == 0x1007 {
        tracing::debug!("obex port 0x1007 detected from device - assuming ios");
        state.device_type = DeviceType::Ios;
        art = Some(CoverArt::init(
          client_man.clone(),
          cover_art_cache,
          cancel_token.child_token(),
          device.address(),
        ));
      } else if obex_port == 0x1001 {
        tracing::debug!("obex port 0x1001 detected from device - assuming android");
        state.device_type = DeviceType::Android;
      }
    } else {
      tracing::warn!("could not get obex port for device!");
    }

    Ok(Self {
      client_man,
      player,
      state,
      art,

      rx,
      _change_handle: tokio::spawn(async move { changes.spawn(change_cancel).await }),
      _avrcp_handle: tokio::spawn(async move { ensure_avrcp(conn, path, device, avrcp_cancel).await }),
      cancel_token,
    })
  }

  pub async fn recv(&mut self) -> Option<DBusPlayerEvent> {
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

  pub async fn shuffle(&self, shuffle: DBusPlayerShuffle) -> DBusResult<()> {
    self.player.set_shuffle((&shuffle).into()).await?;
    Ok(())
  }

  pub async fn repeat(&self, repeat: DBusPlayerRepeat) -> DBusResult<()> {
    self.player.set_repeat((&repeat).into()).await?;
    Ok(())
  }

  pub async fn handle_event(&mut self, event: DBusPlayerEvent) -> DBusResult<()> {
    match event {
      DBusPlayerEvent::Status(status) => {
        self.state.status = status;
      }
      DBusPlayerEvent::Track(track) => {
        // if self.state.track.title != track.title {
        //   if let Some(art) = &self.art {
        //     art.fetch(&track.image_id(), None).await;
        //   }
        // }
        self.state.track = track;
      }
      DBusPlayerEvent::Position(position) => {
        self.state.position = position;
      }
      DBusPlayerEvent::Shuffle(shuffle) => {
        self.state.shuffle = shuffle;
      }
      DBusPlayerEvent::Repeat(repeat) => {
        self.state.repeat = repeat;
      }
    }

    self.send_state().await?;

    Ok(())
  }

  pub async fn send_state(&self) -> DBusResult<()> {
    self
      .client_man
      .broadcast(self.state.to_send_state(), ServerEventType::Info)
      .await?;
    self
      .client_man
      .broadcast(self.state.to_send_queue(), ServerEventType::Info)
      .await?;

    Ok(())
  }

  pub async fn request_cover_art(&self, msg_handle: MsgHandle) {
    if let Some(art) = &self.art {
      art.fetch(&self.state.track.image_id(), Some(msg_handle)).await;
    }
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

pub async fn maybe_recv(player: &mut Option<Player>) -> Option<DBusPlayerEvent> {
  let Some(player) = player else {
    return None;
  };

  let res = player.recv().await;
  tracing::trace!("new player message: {:?}", &res);
  if res.is_none() {
    tracing::error!("player stream appears to be closed?? this is probably bad.");
  }

  res
}

#[derive(Debug, Clone, Default)]
pub struct PlayerState {
  pub device_type: DeviceType,
  pub status: DBusPlayerStatus,
  pub track: DBusPlayerTrack,
  pub position: usize,
  pub shuffle: DBusPlayerShuffle,
  pub repeat: DBusPlayerRepeat,
}

impl PlayerState {
  pub fn to_send_state(&self) -> ServerPlayerEvent {
    ServerPlayerEvent::PlayerState {
      context_id: "spotify:context:fake".to_string(),
      context_title: "BridgeThing".to_string(),
      is_paused: self.status == DBusPlayerStatus::Paused,
      playback_options: PlaybackOptions {
        repeat: self.repeat.into(),
        shuffle: self.shuffle.into(),
      },
      playback_position: self.position,
      playback_restrictions: PlaybackRestrictions {
        can_repeat_context: true,
        can_repeat_track: true,
        can_seek: true,
        can_skip_next: true,
        can_skip_prev: true,
        can_toggle_shuffle: true,
      },
      playback_speed: 1.0,
      track: self.track.clone().into(),
    }
  }

  pub fn to_send_queue(&self) -> ServerPlayerEvent {
    ServerPlayerEvent::Queue {
      current: self.track.clone().into(),
      previous: vec![],
      next: vec![],
    }
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
  tx: PlayerTx,

  status: DBusPlayerStream<String>,
  track: DBusPlayerStream<MediaPlayer1Track>,
  position: DBusPlayerStream<u32>,
  shuffle: DBusPlayerStream<String>,
  repeat: DBusPlayerStream<String>,
}

impl DBusPlayerChanges {
  pub async fn init(tx: PlayerTx, player: &MediaPlayer1Proxy<'static>) -> Self {
    Self {
      tx,

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
