use std::{
  collections::{HashMap, HashSet},
  path::{Path, PathBuf},
  time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use libbridgething::AssetRetention;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set};
use sha2::{Digest, Sha256};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_util::bytes::Bytes;

use super::{
  AssetError, CachedAsset, DISK_BUDGET_BYTES, MEMORY_BUDGET_BYTES, TTL_SWEEP_INTERVAL,
  storage::{AssetActiveModel, AssetColumn, AssetEntity},
};

#[derive(Debug, Clone)]
pub enum AssetCacheEvent {
  Ready { id: String },
  Cleared { id: String },
}

#[derive(Debug)]
pub(super) enum Command {
  Insert {
    id: String,
    bytes: Bytes,
    mime: Option<String>,
    retention: AssetRetention,
    ack: oneshot::Sender<Result<(), AssetError>>,
  },
  InsertFromPath {
    id: String,
    source: PathBuf,
    mime: Option<String>,
    retention: AssetRetention,
    ack: oneshot::Sender<Result<(), AssetError>>,
  },
  Get {
    id: String,
    reply: oneshot::Sender<Option<CachedAsset>>,
  },
  Clear {
    id: String,
    ack: oneshot::Sender<Result<(), AssetError>>,
  },
  ClearAll {
    ack: oneshot::Sender<Result<(), AssetError>>,
  },
}

#[derive(Debug)]
enum EntryStorage {
  Memory(Bytes),
  PersistentFile(PathBuf),
}

#[derive(Debug)]
struct Entry {
  retention: AssetRetention,
  mime: Option<String>,
  byte_len: usize,
  accessed_at: i64,
  lru_seq: u64,
  ttl_deadline: Option<Instant>,
  storage: EntryStorage,
}

pub(super) struct AssetActor {
  blobs_dir: PathBuf,
  entries: HashMap<String, Entry>,
  memory_byte_total: usize,
  disk_byte_total: usize,
  lru_clock: u64,
  dirty_persist: HashSet<String>,
  db: DatabaseConnection,
  cmd_rx: mpsc::Receiver<Command>,
  events_tx: broadcast::Sender<AssetCacheEvent>,
}

impl AssetActor {
  pub(super) fn new(
    db: DatabaseConnection,
    blobs_dir: PathBuf,
    cmd_rx: mpsc::Receiver<Command>,
    events_tx: broadcast::Sender<AssetCacheEvent>,
  ) -> Self {
    Self {
      blobs_dir,
      entries: HashMap::new(),
      memory_byte_total: 0,
      disk_byte_total: 0,
      lru_clock: 0,
      dirty_persist: HashSet::new(),
      db,
      cmd_rx,
      events_tx,
    }
  }

  fn next_lru_seq(&mut self) -> u64 {
    self.lru_clock = self.lru_clock.wrapping_add(1);
    self.lru_clock
  }

  pub(super) async fn bootstrap(mut self) -> Result<Self, AssetError> {
    tokio::fs::create_dir_all(&self.blobs_dir).await.ok();
    let rows = AssetEntity::find()
      .order_by_asc(AssetColumn::AccessedAt)
      .all(&self.db)
      .await?;

    for row in rows {
      let path = PathBuf::from(&row.path);
      if !path.exists() {
        tracing::warn!(id = %row.id, path = %row.path, "asset cache: persistent row references missing file; deleting row");
        let _ = AssetEntity::delete_by_id(row.id.clone()).exec(&self.db).await;
        continue;
      }
      let byte_len = row.byte_len.max(0) as usize;
      self.disk_byte_total = self.disk_byte_total.saturating_add(byte_len);
      let lru_seq = self.next_lru_seq();
      self.entries.insert(
        row.id,
        Entry {
          retention: AssetRetention::Persistent,
          mime: row.mime,
          byte_len,
          accessed_at: row.accessed_at,
          lru_seq,
          ttl_deadline: None,
          storage: EntryStorage::PersistentFile(path),
        },
      );
    }
    self.evict_until_under_disk_budget().await;
    tracing::debug!(
      entries = self.entries.len(),
      disk_bytes = self.disk_byte_total,
      "asset cache bootstrapped"
    );
    Ok(self)
  }

  pub(super) async fn run(mut self) {
    let mut sweep = tokio::time::interval(TTL_SWEEP_INTERVAL);
    sweep.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
      tokio::select! {
        cmd = self.cmd_rx.recv() => match cmd {
          Some(cmd) => self.handle(cmd).await,
          None => {
            tracing::debug!("asset cache actor: command channel closed, exiting");
            return;
          }
        },
        _ = sweep.tick() => {
          self.ttl_sweep();
          self.flush_persist_touches().await;
        }
      }
    }
  }

  async fn handle(&mut self, cmd: Command) {
    match cmd {
      Command::Insert {
        id,
        bytes,
        mime,
        retention,
        ack,
      } => {
        let result = self.handle_insert_memory(id, bytes, mime, retention).await;
        let _ = ack.send(result);
      }
      Command::InsertFromPath {
        id,
        source,
        mime,
        retention,
        ack,
      } => {
        let result = self.handle_insert_from_path(id, source, mime, retention).await;
        let _ = ack.send(result);
      }
      Command::Get { id, reply } => {
        let result = self.handle_get(id).await;
        let _ = reply.send(result);
      }
      Command::Clear { id, ack } => {
        let result = self.handle_clear(id).await;
        let _ = ack.send(result);
      }
      Command::ClearAll { ack } => {
        let result = self.handle_clear_all().await;
        let _ = ack.send(result);
      }
    }
  }

  async fn handle_insert_memory(
    &mut self,
    id: String,
    bytes: Bytes,
    mime: Option<String>,
    retention: AssetRetention,
  ) -> Result<(), AssetError> {
    if matches!(retention, AssetRetention::Persistent) {
      return Err(AssetError::PersistentRequiresChunkedPath);
    }
    let byte_len = bytes.len();
    let now = unix_now();

    self.evict_entry(&id).await;

    let ttl_deadline = match retention {
      AssetRetention::Ttl(t) => Some(Instant::now() + Duration::from_secs(t.seconds.max(1) as u64)),
      _ => None,
    };

    self.memory_byte_total = self.memory_byte_total.saturating_add(byte_len);
    let lru_seq = self.next_lru_seq();
    self.entries.insert(
      id.clone(),
      Entry {
        retention,
        mime,
        byte_len,
        accessed_at: now,
        lru_seq,
        ttl_deadline,
        storage: EntryStorage::Memory(bytes),
      },
    );

    self.evict_until_under_memory_budget().await;
    let _ = self.events_tx.send(AssetCacheEvent::Ready { id });
    Ok(())
  }

  async fn handle_insert_from_path(
    &mut self,
    id: String,
    source: PathBuf,
    mime: Option<String>,
    retention: AssetRetention,
  ) -> Result<(), AssetError> {
    let byte_len = tokio::fs::metadata(&source).await?.len() as usize;
    let now = unix_now();

    self.evict_entry(&id).await;

    if matches!(retention, AssetRetention::Persistent) {
      let dest = self.blobs_dir.join(safe_blob_name(&id));
      if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await.ok();
      }
      if dest.exists() {
        tokio::fs::remove_file(&dest).await.ok();
      }
      rename_or_copy(&source, &dest).await?;
      self.disk_byte_total = self.disk_byte_total.saturating_add(byte_len);
      self.persist_write(&id, &dest, mime.as_deref(), byte_len, now).await?;
      let lru_seq = self.next_lru_seq();
      self.entries.insert(
        id.clone(),
        Entry {
          retention,
          mime,
          byte_len,
          accessed_at: now,
          lru_seq,
          ttl_deadline: None,
          storage: EntryStorage::PersistentFile(dest),
        },
      );
      self.evict_until_under_disk_budget().await;
    } else {
      let bytes = tokio::fs::read(&source).await?;
      tokio::fs::remove_file(&source).await.ok();
      let bytes = Bytes::from(bytes);
      let ttl_deadline = match retention {
        AssetRetention::Ttl(t) => Some(Instant::now() + Duration::from_secs(t.seconds.max(1) as u64)),
        _ => None,
      };
      self.memory_byte_total = self.memory_byte_total.saturating_add(byte_len);
      let lru_seq = self.next_lru_seq();
      self.entries.insert(
        id.clone(),
        Entry {
          retention,
          mime,
          byte_len,
          accessed_at: now,
          lru_seq,
          ttl_deadline,
          storage: EntryStorage::Memory(bytes),
        },
      );
      self.evict_until_under_memory_budget().await;
    }

    let _ = self.events_tx.send(AssetCacheEvent::Ready { id });
    Ok(())
  }

  async fn handle_get(&mut self, id: String) -> Option<CachedAsset> {
    let now = unix_now();
    let lru_seq = self.next_lru_seq();

    let (path, mime, retention) = {
      let entry = self.entries.get_mut(&id)?;
      entry.accessed_at = now;
      entry.lru_seq = lru_seq;
      let mime = entry.mime.clone();
      let retention = entry.retention;
      match &entry.storage {
        EntryStorage::Memory(bytes) => {
          return Some(CachedAsset {
            bytes: bytes.clone(),
            mime,
            retention,
          });
        }
        EntryStorage::PersistentFile(p) => (p.clone(), mime, retention),
      }
    };

    let raw = match tokio::fs::read(&path).await {
      Ok(b) => b,
      Err(err) => {
        tracing::warn!(?err, id = %id, path = %path.display(), "asset cache: persistent file unreadable");
        return None;
      }
    };

    self.dirty_persist.insert(id);

    Some(CachedAsset {
      bytes: Bytes::from(raw),
      mime,
      retention,
    })
  }

  async fn handle_clear(&mut self, id: String) -> Result<(), AssetError> {
    if self.entries.contains_key(&id) {
      self.evict_entry(&id).await;
      let _ = self.events_tx.send(AssetCacheEvent::Cleared { id });
    }
    Ok(())
  }

  async fn handle_clear_all(&mut self) -> Result<(), AssetError> {
    let ids: Vec<String> = self.entries.keys().cloned().collect();
    for id in ids {
      self.evict_entry(&id).await;
      let _ = self.events_tx.send(AssetCacheEvent::Cleared { id });
    }
    Ok(())
  }

  async fn evict_entry(&mut self, id: &str) -> bool {
    let Some(entry) = self.entries.remove(id) else {
      return false;
    };
    match &entry.storage {
      EntryStorage::Memory(_) => {
        self.memory_byte_total = self.memory_byte_total.saturating_sub(entry.byte_len);
      }
      EntryStorage::PersistentFile(path) => {
        self.disk_byte_total = self.disk_byte_total.saturating_sub(entry.byte_len);
        self.dirty_persist.remove(id);
        let _ = tokio::fs::remove_file(path).await;
        if let Err(err) = AssetEntity::delete_by_id(id.to_string()).exec(&self.db).await {
          tracing::warn!(?err, id = %id, "asset cache: failed to delete persistent row");
        }
      }
    }
    true
  }

  async fn evict_until_under_memory_budget(&mut self) {
    while self.memory_byte_total > MEMORY_BUDGET_BYTES {
      let Some(victim) = self.pick_memory_victim() else {
        tracing::error!(
          total = self.memory_byte_total,
          budget = MEMORY_BUDGET_BYTES,
          "asset cache: memory budget exceeded but no eviction candidate"
        );
        break;
      };
      let pinned_warning = matches!(
        self.entries.get(&victim).map(|e| e.retention),
        Some(AssetRetention::Pinned)
      );
      if pinned_warning {
        tracing::warn!(
          id = %victim,
          total = self.memory_byte_total,
          budget = MEMORY_BUDGET_BYTES,
          "asset cache: evicting pinned asset under emergency memory pressure"
        );
      }
      self.evict_entry(&victim).await;
      let _ = self.events_tx.send(AssetCacheEvent::Cleared { id: victim });
    }
  }

  async fn evict_until_under_disk_budget(&mut self) {
    while self.disk_byte_total > DISK_BUDGET_BYTES {
      let Some(victim) = self.pick_disk_victim() else {
        tracing::error!(
          total = self.disk_byte_total,
          budget = DISK_BUDGET_BYTES,
          "asset cache: disk budget exceeded but no eviction candidate"
        );
        break;
      };
      tracing::warn!(
        id = %victim,
        total = self.disk_byte_total,
        budget = DISK_BUDGET_BYTES,
        "asset cache: evicting persistent asset under disk pressure"
      );
      self.evict_entry(&victim).await;
      let _ = self.events_tx.send(AssetCacheEvent::Cleared { id: victim });
    }
  }

  fn pick_memory_victim(&self) -> Option<String> {
    let mut best_lru: Option<(&str, u64)> = None;
    let mut best_ttl: Option<(&str, u64)> = None;
    let mut best_pinned: Option<(&str, u64)> = None;

    for (id, entry) in &self.entries {
      if !matches!(entry.storage, EntryStorage::Memory(_)) {
        continue;
      }
      match entry.retention {
        AssetRetention::Lru => match best_lru {
          Some((_, t)) if entry.lru_seq >= t => {}
          _ => best_lru = Some((id.as_str(), entry.lru_seq)),
        },
        AssetRetention::Ttl(_) => match best_ttl {
          Some((_, t)) if entry.lru_seq >= t => {}
          _ => best_ttl = Some((id.as_str(), entry.lru_seq)),
        },
        AssetRetention::Pinned => match best_pinned {
          Some((_, t)) if entry.lru_seq >= t => {}
          _ => best_pinned = Some((id.as_str(), entry.lru_seq)),
        },
        AssetRetention::Persistent => {}
      }
    }

    best_lru.or(best_ttl).or(best_pinned).map(|(id, _)| id.to_string())
  }

  fn pick_disk_victim(&self) -> Option<String> {
    self
      .entries
      .iter()
      .filter(|(_, e)| matches!(e.storage, EntryStorage::PersistentFile(_)))
      .min_by_key(|(_, e)| e.lru_seq)
      .map(|(id, _)| id.clone())
  }

  fn ttl_sweep(&mut self) {
    let now = Instant::now();
    let expired: Vec<String> = self
      .entries
      .iter()
      .filter_map(|(id, entry)| match entry.ttl_deadline {
        Some(deadline) if deadline <= now => Some(id.clone()),
        _ => None,
      })
      .collect();
    for id in expired {
      if let Some(entry) = self.entries.remove(&id)
        && matches!(entry.storage, EntryStorage::Memory(_))
      {
        self.memory_byte_total = self.memory_byte_total.saturating_sub(entry.byte_len);
      }
      let _ = self.events_tx.send(AssetCacheEvent::Cleared { id });
    }
  }

  async fn persist_write(
    &self,
    id: &str,
    path: &Path,
    mime: Option<&str>,
    byte_len: usize,
    now: i64,
  ) -> Result<(), AssetError> {
    let model = AssetActiveModel {
      id: Set(id.to_string()),
      path: Set(path.to_string_lossy().into_owned()),
      mime: Set(mime.map(str::to_string)),
      byte_len: Set(byte_len as i64),
      inserted_at: Set(now),
      accessed_at: Set(now),
    };
    AssetEntity::insert(model)
      .on_conflict(
        sea_orm::sea_query::OnConflict::column(AssetColumn::Id)
          .update_columns([
            AssetColumn::Path,
            AssetColumn::Mime,
            AssetColumn::ByteLen,
            AssetColumn::InsertedAt,
            AssetColumn::AccessedAt,
          ])
          .to_owned(),
      )
      .exec(&self.db)
      .await?;
    Ok(())
  }

  async fn flush_persist_touches(&mut self) {
    if self.dirty_persist.is_empty() {
      return;
    }
    let ids: Vec<String> = self.dirty_persist.drain().collect();
    let now = unix_now();
    if let Err(err) = AssetEntity::update_many()
      .col_expr(AssetColumn::AccessedAt, sea_orm::sea_query::Expr::value(now))
      .filter(AssetColumn::Id.is_in(ids))
      .exec(&self.db)
      .await
    {
      tracing::warn!(?err, "asset cache: failed to flush persistent accessed_at batch");
    }
  }
}

fn safe_blob_name(id: &str) -> String {
  let mut h = Sha256::new();
  h.update(id.as_bytes());
  hex::encode(h.finalize())
}

async fn rename_or_copy(source: &Path, dest: &Path) -> Result<(), AssetError> {
  match tokio::fs::rename(source, dest).await {
    Ok(()) => Ok(()),
    Err(_) => {
      tokio::fs::copy(source, dest).await?;
      tokio::fs::remove_file(source).await.ok();
      Ok(())
    }
  }
}

fn unix_now() -> i64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|d| d.as_secs() as i64)
    .unwrap_or(0)
}

#[cfg(test)]
mod tests {
  use libbridgething::TtlRetention;

  use super::*;

  fn temp_blobs() -> PathBuf {
    let p = std::env::temp_dir().join(format!("bridgething-asset-test-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&p).unwrap();
    p
  }

  async fn fresh() -> AssetActor {
    let db = crate::db::open(None).await.unwrap();
    let (events_tx, _) = broadcast::channel(16);
    let (_cmd_tx, cmd_rx) = mpsc::channel(16);
    AssetActor::new(db, temp_blobs(), cmd_rx, events_tx)
      .bootstrap()
      .await
      .unwrap()
  }

  #[tokio::test]
  async fn lru_insert_and_get_round_trips() {
    let mut a = fresh().await;
    a.handle_insert_memory(
      "a/1".into(),
      Bytes::from_static(b"hello"),
      Some("text/plain".into()),
      AssetRetention::Lru,
    )
    .await
    .unwrap();
    let got = a.handle_get("a/1".into()).await.unwrap();
    assert_eq!(&got.bytes[..], b"hello");
    assert_eq!(got.mime.as_deref(), Some("text/plain"));
  }

  #[tokio::test]
  async fn ttl_expires_on_sweep() {
    let mut a = fresh().await;
    a.handle_insert_memory(
      "t/1".into(),
      Bytes::from_static(b"x"),
      None,
      AssetRetention::Ttl(TtlRetention { seconds: 1 }),
    )
    .await
    .unwrap();
    assert!(a.handle_get("t/1".into()).await.is_some());
    if let Some(e) = a.entries.get_mut("t/1") {
      e.ttl_deadline = Some(Instant::now() - Duration::from_secs(1));
    }
    a.ttl_sweep();
    assert!(a.handle_get("t/1".into()).await.is_none());
  }

  #[tokio::test]
  async fn persistent_via_insert_from_path_survives_restart() {
    let blobs = temp_blobs();
    let db = crate::db::open(None).await.unwrap();
    let (events_tx, _) = broadcast::channel(16);
    let (_cmd_tx, cmd_rx) = mpsc::channel(16);
    let mut a = AssetActor::new(db.clone(), blobs.clone(), cmd_rx, events_tx)
      .bootstrap()
      .await
      .unwrap();

    let source = std::env::temp_dir().join(format!("src-{}", uuid::Uuid::now_v7()));
    tokio::fs::write(&source, b"persistent-payload").await.unwrap();
    a.handle_insert_from_path(
      "p/1".into(),
      source,
      Some("application/octet-stream".into()),
      AssetRetention::Persistent,
    )
    .await
    .unwrap();
    drop(a);

    let (events_tx2, _) = broadcast::channel(16);
    let (_cmd_tx2, cmd_rx2) = mpsc::channel(16);
    let mut a2 = AssetActor::new(db, blobs, cmd_rx2, events_tx2)
      .bootstrap()
      .await
      .unwrap();
    let got = a2.handle_get("p/1".into()).await.unwrap();
    assert_eq!(&got.bytes[..], b"persistent-payload");
  }

  #[tokio::test]
  async fn lru_eviction_under_memory_pressure() {
    let mut a = fresh().await;
    let big = Bytes::from(vec![0u8; MEMORY_BUDGET_BYTES / 2]);
    a.handle_insert_memory("a".into(), big.clone(), None, AssetRetention::Lru)
      .await
      .unwrap();
    a.handle_insert_memory("b".into(), big.clone(), None, AssetRetention::Lru)
      .await
      .unwrap();
    a.handle_insert_memory("c".into(), big.clone(), None, AssetRetention::Lru)
      .await
      .unwrap();
    assert!(a.handle_get("a".into()).await.is_none());
    assert!(a.handle_get("c".into()).await.is_some());
  }

  #[tokio::test]
  async fn pinned_survives_lru_pressure() {
    let mut a = fresh().await;
    let big = Bytes::from(vec![0u8; MEMORY_BUDGET_BYTES / 2]);
    a.handle_insert_memory("pin".into(), big.clone(), None, AssetRetention::Pinned)
      .await
      .unwrap();
    a.handle_insert_memory("a".into(), big.clone(), None, AssetRetention::Lru)
      .await
      .unwrap();
    a.handle_insert_memory("b".into(), big.clone(), None, AssetRetention::Lru)
      .await
      .unwrap();
    assert!(a.handle_get("pin".into()).await.is_some());
    assert!(a.handle_get("a".into()).await.is_none());
  }

  #[tokio::test]
  async fn insert_memory_with_persistent_retention_rejected() {
    let mut a = fresh().await;
    let err = a
      .handle_insert_memory(
        "p/x".into(),
        Bytes::from_static(b"nope"),
        None,
        AssetRetention::Persistent,
      )
      .await
      .unwrap_err();
    assert!(matches!(err, AssetError::PersistentRequiresChunkedPath));
  }
}
