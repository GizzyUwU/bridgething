//! Generic asset cache. Single source of truth for binary blobs
//! addressable by opaque string id, regardless of who produced them.
//!
//! Producers: companion-pushed (`GatewayToBridgeAssetMsg::Push`),
//! iAP2 FileTransfer (`iap2/art/...` ids), or any future daemon-internal
//! source that wants a place to put bytes a webapp will fetch later.
//!
//! Consumers: webapps via `ClientAssetCommand::Get`, stock GetImage via
//! the legacy player-image WS event, and direct in-process callers.
//!
//! Retention follows `AssetRetention`: `Lru` participates in a global
//! memory budget with oldest-accessed eviction; `Pinned` is held until
//! `Clear`; `Ttl` auto-expires after a duration; `Persistent` writes
//! through to sqlite and survives daemon restart.
//!
//! The cache is single-task-owned (actor pattern); the public
//! [`AssetCache`] handle is `Clone` and posts commands across an mpsc
//! to the owning task. Subscribe to [`AssetCacheEvent`]s with
//! `subscribe()` for `Ready` / `Cleared` notifications.

mod actor;
pub mod storage;

use std::{path::PathBuf, sync::Arc, time::Duration};

pub use actor::AssetCacheEvent;
use libbridgething::AssetRetention;
use sea_orm::DbErr;
use tokio::{
  sync::{broadcast, mpsc, oneshot},
  task::JoinHandle,
};
use tokio_util::bytes::Bytes;

/// Total in-memory bytes across `Lru` / `Pinned` / `Ttl` entries.
/// Persistent in-memory copies don't count - their bound is `DISK_BUDGET_BYTES`.
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
/// [`AssetCache::init`], spawn the owning task with [`AssetCache::spawn`].
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
  /// Open the database, run migrations, and prepare the cache. The
  /// returned [`AssetCachePending`] holds the actor task; call
  /// [`AssetCachePending::spawn`] when the daemon is ready to begin
  /// serving cache traffic.
  pub async fn init(db_path: PathBuf) -> Result<AssetCachePending, AssetError> {
    let db = storage::open_db(&db_path).await?;

    let (cmd_tx, cmd_rx) = mpsc::channel(COMMAND_MAILBOX_CAPACITY);
    let (events_tx, _) = broadcast::channel(EVENT_BROADCAST_CAPACITY);

    let actor = actor::AssetActor::new(db, cmd_rx, events_tx.clone());
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

  /// Subscribe to cache events. Returns a fresh receiver so each
  /// caller sees events from the moment of subscribe forward.
  pub fn subscribe(&self) -> broadcast::Receiver<AssetCacheEvent> {
    self.inner.events_tx.subscribe()
  }
}

/// Initialised but not-yet-running cache. Call [`spawn`] to start the
/// actor task.
///
/// [`spawn`]: AssetCachePending::spawn
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
}
