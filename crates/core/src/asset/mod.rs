//! Generic asset cache. Single source of truth for binary blobs
//! addressable by opaque string id, regardless of who produced them.
//!
//! Producers: companion-pushed (single-frame `GatewayToBridgeAssetMsg::
//! Push` for memory-resident small blobs, or chunked `PushBegin`/
//! `PushChunk` via `ChunkedTransfer` for anything Persistent or larger
//! than 256 KB), iAP2 FileTransfer (`iap2/art/...` ids), or any future
//! daemon-internal source that wants a place to put bytes a webapp
//! will fetch later.
//!
//! Consumers: webapps via `ClientToBridgeAssetMsg::Get`, stock GetImage
//! via the legacy player-image WS event, and direct in-process callers.
//!
//! Retention follows `AssetRetention`: `Lru` participates in a global
//! memory budget with oldest-accessed eviction; `Pinned` is held until
//! `Clear`; `Ttl` auto-expires after a duration; `Persistent` writes
//! through to a per-id file under `paths::assets_blobs_dir()` and
//! survives daemon restart. Persistent entries are read from disk on
//! every `Get`; their bytes never sit in the daemon's memory budget.
//!
//! The cache is single-task-owned (actor pattern); the public
//! [`AssetCache`] handle is `Clone` and posts commands across an mpsc
//! to the owning task. Subscribe to [`AssetCacheEvent`]s with
//! `subscribe()` for `Ready` / `Cleared` notifications.

mod actor;
pub mod ingest;
pub mod storage;
pub mod wait;

use std::{path::PathBuf, sync::Arc, time::Duration};

pub use actor::AssetCacheEvent;
pub use ingest::AssetIngest;
use libbridgething::AssetRetention;
use sea_orm::{DatabaseConnection, DbErr};
use tokio::{
  sync::{broadcast, mpsc, oneshot},
  task::JoinHandle,
};
use tokio_util::bytes::Bytes;

pub const MEMORY_BUDGET_BYTES: usize = 8 * 1024 * 1024;
pub const DISK_BUDGET_BYTES: usize = 50 * 1024 * 1024;
const TTL_SWEEP_INTERVAL: Duration = Duration::from_secs(15);
const EVENT_BROADCAST_CAPACITY: usize = 64;
const COMMAND_MAILBOX_CAPACITY: usize = 16;

#[derive(Debug, Clone)]
pub struct CachedAsset {
  pub bytes: Bytes,
  pub mime: Option<String>,
  pub retention: AssetRetention,
}

#[derive(Debug, Clone)]
pub struct AssetCache {
  inner: Arc<AssetCacheInner>,
}

#[derive(Debug)]
struct AssetCacheInner {
  cmd_tx: mpsc::Sender<actor::Command>,
  events_tx: broadcast::Sender<AssetCacheEvent>,
}

impl AssetCache {
  pub async fn init(db: DatabaseConnection, blobs_dir: PathBuf) -> Result<AssetCachePending, AssetError> {
    let (cmd_tx, cmd_rx) = mpsc::channel(COMMAND_MAILBOX_CAPACITY);
    let (events_tx, _) = broadcast::channel(EVENT_BROADCAST_CAPACITY);

    let actor = actor::AssetActor::new(db, blobs_dir, cmd_rx, events_tx.clone());
    let actor = actor.bootstrap().await?;

    Ok(AssetCachePending {
      actor,
      handle: Self {
        inner: Arc::new(AssetCacheInner { cmd_tx, events_tx }),
      },
    })
  }

  pub async fn insert(
    &self,
    id: String,
    bytes: Bytes,
    mime: Option<String>,
    retention: AssetRetention,
  ) -> Result<(), AssetError> {
    let (ack, rx) = oneshot::channel();
    self
      .inner
      .cmd_tx
      .send(actor::Command::Insert {
        id,
        bytes,
        mime,
        retention,
        ack,
      })
      .await
      .map_err(|_| AssetError::CacheClosed)?;
    rx.await.map_err(|_| AssetError::CacheClosed)?
  }

  pub async fn insert_from_path(
    &self,
    id: String,
    source: PathBuf,
    mime: Option<String>,
    retention: AssetRetention,
  ) -> Result<(), AssetError> {
    let (ack, rx) = oneshot::channel();
    self
      .inner
      .cmd_tx
      .send(actor::Command::InsertFromPath {
        id,
        source,
        mime,
        retention,
        ack,
      })
      .await
      .map_err(|_| AssetError::CacheClosed)?;
    rx.await.map_err(|_| AssetError::CacheClosed)?
  }

  pub async fn get(&self, id: &str) -> Result<Option<CachedAsset>, AssetError> {
    let (reply, rx) = oneshot::channel();
    self
      .inner
      .cmd_tx
      .send(actor::Command::Get {
        id: id.to_string(),
        reply,
      })
      .await
      .map_err(|_| AssetError::CacheClosed)?;
    rx.await.map_err(|_| AssetError::CacheClosed)
  }

  pub async fn clear_all(&self) -> Result<(), AssetError> {
    let (ack, rx) = oneshot::channel();
    self
      .inner
      .cmd_tx
      .send(actor::Command::ClearAll { ack })
      .await
      .map_err(|_| AssetError::CacheClosed)?;
    rx.await.map_err(|_| AssetError::CacheClosed)?
  }

  pub async fn clear(&self, id: &str) -> Result<(), AssetError> {
    let (ack, rx) = oneshot::channel();
    self
      .inner
      .cmd_tx
      .send(actor::Command::Clear {
        id: id.to_string(),
        ack,
      })
      .await
      .map_err(|_| AssetError::CacheClosed)?;
    rx.await.map_err(|_| AssetError::CacheClosed)?
  }

  pub fn subscribe(&self) -> broadcast::Receiver<AssetCacheEvent> {
    self.inner.events_tx.subscribe()
  }
}

pub struct AssetCachePending {
  actor: actor::AssetActor,
  handle: AssetCache,
}

impl AssetCachePending {
  pub fn spawn(self) -> (AssetCache, JoinHandle<()>) {
    let join = tokio::spawn(self.actor.run());
    (self.handle, join)
  }
}

pub type AssetResult<T> = Result<T, AssetError>;

#[derive(Debug, thiserror::Error)]
pub enum AssetError {
  #[error("asset cache database error: {0}")]
  Db(#[from] DbErr),
  #[error("asset cache actor channel closed")]
  CacheClosed,
  #[error("asset cache io error: {0}")]
  Io(#[from] std::io::Error),
  #[error("Persistent retention requires the chunked PushBegin/PushChunk path; single-frame Push is memory-only")]
  PersistentRequiresChunkedPath,
}
