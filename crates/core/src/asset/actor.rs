use std::{
  collections::HashMap,
  time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use libbridgething::AssetRetention;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_util::bytes::Bytes;

use super::{
  AssetError, CachedAsset, DISK_BUDGET_BYTES, MEMORY_BUDGET_BYTES, TTL_SWEEP_INTERVAL,
  storage::{AssetActiveModel, AssetColumn, AssetEntity},
};

/// Notifications broadcast on every cache mutation. Webapps and SDK
/// consumers subscribe through their existing WS event stream;
/// in-process consumers (e.g. iap2 art writer) can subscribe directly.
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
  Get {
    id: String,
    reply: oneshot::Sender<Option<CachedAsset>>,
  },
  Clear {
    id: String,
    ack: oneshot::Sender<Result<(), AssetError>>,
  },
}

#[derive(Debug)]
struct Entry {
  bytes: Option<Bytes>,
  mime: Option<String>,
  retention: AssetRetention,
  byte_len: usize,
  /// Wall-clock unix seconds; persists into the assets table so disk
  /// LRU survives daemon restart.
  accessed_at: i64,
  /// Monotonic counter bumped on every insert/access. Used purely for
  /// in-memory LRU victim selection - distinguishes inserts that fall
  /// in the same wall-clock second.
  lru_seq: u64,
  ttl_deadline: Option<Instant>,
}

pub(super) struct AssetActor {
  entries: HashMap<String, Entry>,
  memory_byte_total: usize,
  disk_byte_total: usize,
  /// Monotonic LRU counter. Bumped before each Entry assigns it.
  lru_clock: u64,
  db: DatabaseConnection,
  cmd_rx: mpsc::Receiver<Command>,
  events_tx: broadcast::Sender<AssetCacheEvent>,
}

impl AssetActor {
  pub(super) fn new(
    db: DatabaseConnection,
    cmd_rx: mpsc::Receiver<Command>,
    events_tx: broadcast::Sender<AssetCacheEvent>,
  ) -> Self {
    Self {
      entries: HashMap::new(),
      memory_byte_total: 0,
      disk_byte_total: 0,
      lru_clock: 0,
      db,
      cmd_rx,
      events_tx,
    }
  }

  fn next_lru_seq(&mut self) -> u64 {
    self.lru_clock = self.lru_clock.wrapping_add(1);
    self.lru_clock
  }

  /// Scan persistent storage and populate the in-memory index of
  /// `(id, byte_len, accessed_at)` so the cache "knows" about persisted
  /// assets at startup without loading their bytes. First `Get` for
  /// each lazy-loads from disk.
  pub(super) async fn bootstrap(mut self) -> Result<Self, AssetError> {
    let rows = AssetEntity::find()
      .order_by_asc(AssetColumn::AccessedAt)
      .all(&self.db)
      .await?;

    for row in rows {
      let byte_len = row.byte_len.max(0) as usize;
      self.disk_byte_total = self.disk_byte_total.saturating_add(byte_len);
      let lru_seq = self.next_lru_seq();
      self.entries.insert(
        row.id,
        Entry {
          bytes: None,
          mime: row.mime,
          retention: AssetRetention::Persistent,
          byte_len,
          accessed_at: row.accessed_at,
          lru_seq,
          ttl_deadline: None,
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
        _ = sweep.tick() => self.ttl_sweep(),
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
        let result = self.handle_insert(id, bytes, mime, retention).await;
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
    }
  }

  async fn handle_insert(
    &mut self,
    id: String,
    bytes: Bytes,
    mime: Option<String>,
    retention: AssetRetention,
  ) -> Result<(), AssetError> {
    let byte_len = bytes.len();
    let now = unix_now();

    // drop any prior entry first so byte counters are consistent
    self.evict_entry(&id).await;

    let ttl_deadline = match retention {
      AssetRetention::Ttl(t) => Some(Instant::now() + Duration::from_secs(t.seconds.max(1) as u64)),
      _ => None,
    };

    if matches!(retention, AssetRetention::Persistent) {
      self.persist_write(&id, &bytes, mime.as_deref(), now).await?;
      self.disk_byte_total = self.disk_byte_total.saturating_add(byte_len);
    } else {
      self.memory_byte_total = self.memory_byte_total.saturating_add(byte_len);
    }

    let lru_seq = self.next_lru_seq();
    self.entries.insert(
      id.clone(),
      Entry {
        bytes: Some(bytes),
        mime,
        retention,
        byte_len,
        accessed_at: now,
        lru_seq,
        ttl_deadline,
      },
    );

    self.evict_until_under_memory_budget().await;
    if matches!(retention, AssetRetention::Persistent) {
      self.evict_until_under_disk_budget().await;
    }

    let _ = self.events_tx.send(AssetCacheEvent::Ready { id });
    Ok(())
  }

  async fn handle_get(&mut self, id: String) -> Option<CachedAsset> {
    let now = unix_now();
    let lru_seq = self.next_lru_seq();

    // fast path: entry present with bytes
    let fast = self.entries.get_mut(&id).and_then(|entry| {
      let bytes = entry.bytes.clone()?;
      entry.accessed_at = now;
      entry.lru_seq = lru_seq;
      Some((bytes, entry.mime.clone(), entry.retention))
    });
    if let Some((bytes, mime, retention)) = fast {
      if matches!(retention, AssetRetention::Persistent) {
        let _ = self.touch_persist(&id, now).await;
      }
      return Some(CachedAsset { bytes, mime, retention });
    }

    // lazy-load path: persistent index entry without loaded bytes
    let needs_load = self
      .entries
      .get(&id)
      .map(|e| e.bytes.is_none() && matches!(e.retention, AssetRetention::Persistent))
      .unwrap_or(false);

    if !needs_load {
      return None;
    }

    let row = match AssetEntity::find_by_id(id.clone()).one(&self.db).await {
      Ok(Some(row)) => row,
      Ok(None) => {
        // index drift - drop the stale index entry
        if let Some(entry) = self.entries.remove(&id) {
          self.disk_byte_total = self.disk_byte_total.saturating_sub(entry.byte_len);
        }
        return None;
      }
      Err(err) => {
        tracing::warn!(?err, id = %id, "asset cache: lazy load failed");
        return None;
      }
    };

    let bytes = Bytes::from(row.bytes);
    let mime = row.mime;
    if let Some(entry) = self.entries.get_mut(&id) {
      entry.bytes = Some(bytes.clone());
      entry.mime = mime.clone();
      entry.accessed_at = now;
      entry.lru_seq = lru_seq;
    }
    let _ = self.touch_persist(&id, now).await;

    Some(CachedAsset {
      bytes,
      mime,
      retention: AssetRetention::Persistent,
    })
  }

  async fn handle_clear(&mut self, id: String) -> Result<(), AssetError> {
    if self.entries.contains_key(&id) {
      self.evict_entry(&id).await;
      let _ = self.events_tx.send(AssetCacheEvent::Cleared { id });
    }
    Ok(())
  }

  /// Drop one entry, updating byte counters and removing from disk if
  /// the entry was persistent. Does not broadcast `Cleared`; callers
  /// decide whether the eviction is user-visible.
  async fn evict_entry(&mut self, id: &str) -> bool {
    let Some(entry) = self.entries.remove(id) else {
      return false;
    };
    if matches!(entry.retention, AssetRetention::Persistent) {
      self.disk_byte_total = self.disk_byte_total.saturating_sub(entry.byte_len);
      if let Err(err) = AssetEntity::delete_by_id(id.to_string()).exec(&self.db).await {
        tracing::warn!(?err, id = %id, "asset cache: failed to delete persistent row");
      }
    } else if entry.bytes.is_some() {
      self.memory_byte_total = self.memory_byte_total.saturating_sub(entry.byte_len);
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

  /// Eviction order across memory: oldest `Lru` first, then oldest
  /// `Ttl`, then oldest `Pinned` (with warn-log emitted at the call
  /// site). `Persistent` is never memory-evicted; its in-memory copy
  /// can be dropped lazily but the entry remains.
  fn pick_memory_victim(&self) -> Option<String> {
    let mut best_lru: Option<(&str, u64)> = None;
    let mut best_ttl: Option<(&str, u64)> = None;
    let mut best_pinned: Option<(&str, u64)> = None;

    for (id, entry) in &self.entries {
      if entry.bytes.is_none() {
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
      .filter(|(_, e)| matches!(e.retention, AssetRetention::Persistent))
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
      if let Some(entry) = self.entries.remove(&id) {
        self.memory_byte_total = self.memory_byte_total.saturating_sub(entry.byte_len);
      }
      let _ = self.events_tx.send(AssetCacheEvent::Cleared { id });
    }
  }

  async fn persist_write(&self, id: &str, bytes: &Bytes, mime: Option<&str>, now: i64) -> Result<(), AssetError> {
    let model = AssetActiveModel {
      id: Set(id.to_string()),
      bytes: Set(bytes.to_vec()),
      mime: Set(mime.map(str::to_string)),
      byte_len: Set(bytes.len() as i64),
      inserted_at: Set(now),
      accessed_at: Set(now),
    };
    AssetEntity::insert(model)
      .on_conflict(
        sea_orm::sea_query::OnConflict::column(AssetColumn::Id)
          .update_columns([
            AssetColumn::Bytes,
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

  async fn touch_persist(&self, id: &str, now: i64) -> Result<(), AssetError> {
    AssetEntity::update_many()
      .col_expr(AssetColumn::AccessedAt, sea_orm::sea_query::Expr::value(now))
      .filter(AssetColumn::Id.eq(id))
      .exec(&self.db)
      .await?;
    Ok(())
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

  async fn fresh() -> AssetActor {
    let db = crate::db::open(None).await.unwrap();
    let (events_tx, _) = broadcast::channel(16);
    let (_cmd_tx, cmd_rx) = mpsc::channel(16);
    AssetActor::new(db, cmd_rx, events_tx).bootstrap().await.unwrap()
  }

  #[tokio::test]
  async fn lru_insert_and_get_round_trips() {
    let mut a = fresh().await;
    a.handle_insert(
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
    a.handle_insert(
      "t/1".into(),
      Bytes::from_static(b"x"),
      None,
      AssetRetention::Ttl(TtlRetention { seconds: 1 }),
    )
    .await
    .unwrap();
    assert!(a.handle_get("t/1".into()).await.is_some());
    // fast-forward by mutating the deadline
    if let Some(e) = a.entries.get_mut("t/1") {
      e.ttl_deadline = Some(Instant::now() - Duration::from_secs(1));
    }
    a.ttl_sweep();
    assert!(a.handle_get("t/1".into()).await.is_none());
  }

  #[tokio::test]
  async fn persistent_survives_evict_via_disk() {
    let mut a = fresh().await;
    a.handle_insert(
      "p/1".into(),
      Bytes::from_static(b"persistent"),
      Some("application/octet-stream".into()),
      AssetRetention::Persistent,
    )
    .await
    .unwrap();
    // evict from memory map (simulate restart) but keep the db
    let db = a.db.clone();
    drop(a);
    let (events_tx, _) = broadcast::channel(16);
    let (_cmd_tx, cmd_rx) = mpsc::channel(16);
    let mut a2 = AssetActor::new(db, cmd_rx, events_tx).bootstrap().await.unwrap();
    let got = a2.handle_get("p/1".into()).await.unwrap();
    assert_eq!(&got.bytes[..], b"persistent");
  }

  #[tokio::test]
  async fn lru_eviction_under_memory_pressure() {
    let mut a = fresh().await;
    let big = Bytes::from(vec![0u8; MEMORY_BUDGET_BYTES / 2]);
    a.handle_insert("a".into(), big.clone(), None, AssetRetention::Lru)
      .await
      .unwrap();
    a.handle_insert("b".into(), big.clone(), None, AssetRetention::Lru)
      .await
      .unwrap();
    a.handle_insert("c".into(), big.clone(), None, AssetRetention::Lru)
      .await
      .unwrap();
    // c's insert pushed total over budget; oldest (a) should be evicted
    assert!(a.handle_get("a".into()).await.is_none());
    assert!(a.handle_get("c".into()).await.is_some());
  }

  #[tokio::test]
  async fn pinned_survives_lru_pressure() {
    let mut a = fresh().await;
    let big = Bytes::from(vec![0u8; MEMORY_BUDGET_BYTES / 2]);
    a.handle_insert("pin".into(), big.clone(), None, AssetRetention::Pinned)
      .await
      .unwrap();
    a.handle_insert("a".into(), big.clone(), None, AssetRetention::Lru)
      .await
      .unwrap();
    a.handle_insert("b".into(), big.clone(), None, AssetRetention::Lru)
      .await
      .unwrap();
    assert!(a.handle_get("pin".into()).await.is_some());
    assert!(a.handle_get("a".into()).await.is_none());
  }
}
