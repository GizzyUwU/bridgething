use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MediaRepeatMode {
  Off,
  One,
  All,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MediaQueueEntry {
  pub queue_id: i64,
  pub title: Option<String>,
  pub subtitle: Option<String>,
  pub art_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MediaSessionSnapshot {
  pub package: String,
  pub title: Option<String>,
  pub artist: Option<String>,
  pub album: Option<String>,
  pub duration_ms: Option<i64>,
  pub position_ms: i64,
  pub playing: bool,
  pub can_seek: bool,
  pub art_token: Option<String>,
  pub queue: Vec<MediaQueueEntry>,
  pub active_queue_id: Option<i64>,
  pub shuffle: Option<bool>,
  pub repeat: Option<MediaRepeatMode>,
  pub speed: Option<f32>,
  pub position_age_ms: Option<i64>,
  pub liked: Option<bool>,
  pub like_supported: bool,
  pub queue_title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MediaArt {
  pub bytes: Vec<u8>,
  pub mime: String,
}

#[derive(Debug, Clone, Copy, PartialEq, uniffi::Enum)]
pub enum MediaControl {
  Play,
  Pause,
  SkipNext,
  SkipPrev,
  SeekTo { position_ms: i64 },
  SkipToQueueItem { queue_id: i64 },
  SetShuffle { on: bool },
  SetRepeat { mode: MediaRepeatMode },
  SetSpeed { speed: f32 },
  SetLiked { liked: bool },
}

#[uniffi::export(with_foreign)]
pub trait MediaSessionBackend: Send + Sync {
  fn is_access_granted(&self) -> bool;
  fn start(&self, inbox: Arc<MediaSessionInbox>);
  fn stop(&self);
  fn snapshot_all(&self, sink: Arc<MediaSnapshotSink>);
  fn control(&self, package: String, cmd: MediaControl);
  fn art(&self, package: String, token: String, sink: Arc<MediaArtSink>);
}

#[derive(uniffi::Object)]
pub struct MediaSessionInbox {
  tx: mpsc::UnboundedSender<()>,
}

impl MediaSessionInbox {
  pub fn channel() -> (Arc<Self>, mpsc::UnboundedReceiver<()>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (Arc::new(Self { tx }), rx)
  }
}

#[uniffi::export]
impl MediaSessionInbox {
  pub fn on_sessions_changed(&self) {
    let _ = self.tx.send(());
  }
}

#[derive(uniffi::Object)]
pub struct MediaSnapshotSink {
  tx: std::sync::Mutex<Option<oneshot::Sender<Vec<MediaSessionSnapshot>>>>,
}

impl MediaSnapshotSink {
  pub fn channel() -> (Arc<Self>, oneshot::Receiver<Vec<MediaSessionSnapshot>>) {
    let (tx, rx) = oneshot::channel();
    (
      Arc::new(Self {
        tx: std::sync::Mutex::new(Some(tx)),
      }),
      rx,
    )
  }
}

#[uniffi::export]
impl MediaSnapshotSink {
  pub fn complete(&self, sessions: Vec<MediaSessionSnapshot>) {
    if let Some(tx) = self.tx.lock().unwrap().take() {
      let _ = tx.send(sessions);
    }
  }
}

#[derive(uniffi::Object)]
pub struct MediaArtSink {
  tx: std::sync::Mutex<Option<oneshot::Sender<Option<MediaArt>>>>,
}

impl MediaArtSink {
  pub fn channel() -> (Arc<Self>, oneshot::Receiver<Option<MediaArt>>) {
    let (tx, rx) = oneshot::channel();
    (
      Arc::new(Self {
        tx: std::sync::Mutex::new(Some(tx)),
      }),
      rx,
    )
  }
}

#[uniffi::export]
impl MediaArtSink {
  pub fn complete(&self, art: Option<MediaArt>) {
    if let Some(tx) = self.tx.lock().unwrap().take() {
      let _ = tx.send(art);
    }
  }
}
