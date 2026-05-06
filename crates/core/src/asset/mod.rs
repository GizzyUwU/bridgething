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
pub mod storage;
pub mod wait;

use std::{path::PathBuf, sync::Arc, time::Duration};

pub use actor::AssetCacheEvent;
use libbridgething::AssetRetention;
use sea_orm::{DatabaseConnection, DbErr};
use tokio::{
  sync::{broadcast, mpsc, oneshot},
  task::JoinHandle,
};
use tokio_util::bytes::Bytes;

/// Total in-memory bytes across `Lru` / `Pinned` / `Ttl` entries.
/// Persistent entries don't sit in memory - their bound is
/// `DISK_BUDGET_BYTES`.
pub const MEMORY_BUDGET_BYTES: usize = 8 * 1024 * 1024;

/// Total bytes for `Persistent` entries on disk.
pub const DISK_BUDGET_BYTES: usize = 50 * 1024 * 1024;

/// Periodic sweep cadence for `Ttl` expiry.
const TTL_SWEEP_INTERVAL: Duration = Duration::from_secs(1);

/// Capacity of `AssetCacheEvent` broadcast. Slow subscribers see lag
/// as a missed event, never blocked publishes - the cache is fast-path.
const EVENT_BROADCAST_CAPACITY: usize = 64;

/// Capacity of the actor command mailbox. Backpressure on the
/// producer side; aligns with the rest of the daemon's mpsc(16) default.
const COMMAND_MAILBOX_CAPACITY: usize = 16;

/// Snapshot of an asset at the time of a `get` call. `bytes` is a
/// refcount-cloned `Bytes` so passing it through the WS / iap2 paths
/// is cheap.
#[derive(Debug, Clone)]
pub struct CachedAsset {
  pub bytes: Bytes,
  pub mime: Option<String>,
  pub retention: AssetRetention,
}

/// Cloneable handle to the asset cache actor. Construct one via
/// [`AssetCache::init`], spawn the owning task with [`AssetCachePending::spawn`].
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
  /// Prepare the cache against a pre-opened, already-migrated database
  /// connection. The returned [`AssetCachePending`] holds the actor
  /// task; call [`AssetCachePending::spawn`] when the daemon is ready
  /// to begin serving cache traffic.
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

  /// Memory-tier insert. `retention` must be `Lru` / `Pinned` / `Ttl`;
  /// `Persistent` is rejected because the bytes-in-hand path is
  /// memory-resident by definition. For Persistent assets use
  /// [`AssetCache::insert_from_path`].
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

  /// Disk-tier insert: the source file (typically a finalized
  /// `ChunkedTransfer` partial) is renamed into the blobs dir for
  /// `Persistent` retention, or read into a `Bytes` for memory tiers
  /// then deleted. Either way the source path is consumed.
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
