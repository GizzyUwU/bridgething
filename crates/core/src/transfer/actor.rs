use std::{
  collections::HashMap,
  path::{Path, PathBuf},
  time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::{
  fs::{File, OpenOptions},
  io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
  sync::{mpsc, oneshot},
};
use tokio_util::bytes::Bytes;

use super::{ChunkOutcome, STALE_TIMEOUT, SWEEP_INTERVAL, TRANSFER_DISK_BUDGET_BYTES, TransferError, safe_filename};

#[derive(Debug)]
pub(super) enum Command {
  Begin {
    id: String,
    expected_size: u64,
    expected_sha256: Option<String>,
    ack: oneshot::Sender<Result<u64, TransferError>>,
  },
  AcceptChunk {
    id: String,
    offset: u64,
    bytes: Bytes,
    last: bool,
    ack: oneshot::Sender<Result<ChunkOutcome, TransferError>>,
  },
  Abandon {
    id: String,
    ack: oneshot::Sender<Result<(), TransferError>>,
  },
}

#[derive(Debug, Serialize, Deserialize)]
struct Meta {
  id: String,
  expected_size: u64,
  #[serde(skip_serializing_if = "Option::is_none")]
  expected_sha256: Option<String>,
  received: u64,
  last_touched_unix: i64,
}

#[derive(Debug)]
struct Transfer {
  id: String,
  expected_size: u64,
  expected_sha256: Option<String>,
  received: u64,
  last_touched_unix: i64,
  partial_path: PathBuf,
  meta_path: PathBuf,
}

pub(super) struct ChunkedTransferActor {
  transfers_dir: PathBuf,
  transfers: HashMap<String, Transfer>,
  total_disk_bytes: u64,
  cmd_rx: mpsc::Receiver<Command>,
}

impl ChunkedTransferActor {
  pub(super) async fn bootstrap(
    transfers_dir: PathBuf,
    cmd_rx: mpsc::Receiver<Command>,
  ) -> Result<Self, TransferError> {
    let mut transfers = HashMap::new();
    let mut total = 0u64;

    let mut entries = tokio::fs::read_dir(&transfers_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
      let path = entry.path();
      if path.extension().and_then(|s| s.to_str()) != Some("meta") {
        continue;
      }
      match load_recovered_transfer(&path).await {
        Ok(Some(transfer)) => {
          tracing::debug!(
            id = %transfer.id,
            received = transfer.received,
            expected = transfer.expected_size,
            "transfer: recovered partial on bootstrap",
          );
          total = total.saturating_add(transfer.received);
          transfers.insert(transfer.id.clone(), transfer);
        }
        Ok(None) => {}
        Err(err) => {
          tracing::warn!(?err, meta = %path.display(), "transfer: failed to recover; deleting");
          let _ = tokio::fs::remove_file(&path).await;
          if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            let partial = transfers_dir.join(format!("{stem}.partial"));
            let _ = tokio::fs::remove_file(&partial).await;
          }
        }
      }
    }

    sweep_orphan_partials(&transfers_dir, &transfers).await;

    tracing::info!(
      transfers = transfers.len(),
      bytes = total,
      "chunked transfer actor bootstrapped"
    );

    Ok(Self {
      transfers_dir,
      transfers,
      total_disk_bytes: total,
      cmd_rx,
    })
  }

  pub(super) async fn run(mut self) {
    let mut sweep = tokio::time::interval(SWEEP_INTERVAL);
    sweep.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    tracing::debug!("chunked transfer actor running");
    loop {
      tokio::select! {
        cmd = self.cmd_rx.recv() => match cmd {
          Some(cmd) => self.handle(cmd).await,
          None => {
            tracing::debug!("chunked transfer actor: command channel closed, exiting");
            return;
          }
        },
        _ = sweep.tick() => self.sweep_stale().await,
      }
    }
  }

  async fn handle(&mut self, cmd: Command) {
    match cmd {
      Command::Begin {
        id,
        expected_size,
        expected_sha256,
        ack,
      } => {
        let result = self.handle_begin(id, expected_size, expected_sha256).await;
        let _ = ack.send(result);
      }
      Command::AcceptChunk {
        id,
        offset,
        bytes,
        last,
        ack,
      } => {
        let result = self.handle_chunk(id, offset, bytes, last).await;
        let _ = ack.send(result);
      }
      Command::Abandon { id, ack } => {
        let result = self.handle_abandon(id).await;
        let _ = ack.send(result);
      }
    }
  }

  async fn handle_begin(
    &mut self,
    id: String,
    expected_size: u64,
    expected_sha256: Option<String>,
  ) -> Result<u64, TransferError> {
    if expected_size > TRANSFER_DISK_BUDGET_BYTES {
      return Err(TransferError::TooLarge {
        id,
        size: expected_size,
      });
    }

    if let Some(existing) = self.transfers.get_mut(&id) {
      if existing.expected_size != expected_size || existing.expected_sha256 != expected_sha256 {
        return Err(TransferError::ConflictingBegin { id });
      }
      existing.last_touched_unix = unix_now();
      write_meta(&existing.meta_path, &meta_from(existing)).await?;
      return Ok(existing.received);
    }

    let projected = self.total_disk_bytes.saturating_add(expected_size);
    if projected > TRANSFER_DISK_BUDGET_BYTES {
      self.evict_until_under(expected_size).await;
      let still_projected = self.total_disk_bytes.saturating_add(expected_size);
      if still_projected > TRANSFER_DISK_BUDGET_BYTES {
        return Err(TransferError::BudgetExceeded { id });
      }
    }

    let stem = safe_filename(&id);
    let partial_path = self.transfers_dir.join(format!("{stem}.partial"));
    let meta_path = self.transfers_dir.join(format!("{stem}.meta"));

    let _ = tokio::fs::remove_file(&partial_path).await;
    File::create(&partial_path).await?;

    let transfer = Transfer {
      id: id.clone(),
      expected_size,
      expected_sha256,
      received: 0,
      last_touched_unix: unix_now(),
      partial_path,
      meta_path: meta_path.clone(),
    };
    write_meta(&meta_path, &meta_from(&transfer)).await?;
    self.transfers.insert(id, transfer);
    Ok(0)
  }

  async fn handle_chunk(
    &mut self,
    id: String,
    offset: u64,
    bytes: Bytes,
    last: bool,
  ) -> Result<ChunkOutcome, TransferError> {
    let transfer = self
      .transfers
      .get_mut(&id)
      .ok_or_else(|| TransferError::UnknownTransfer { id: id.clone() })?;

    if offset != transfer.received {
      return Err(TransferError::OffsetMismatch {
        id,
        expected: transfer.received,
        got: offset,
      });
    }
    let chunk_len = bytes.len() as u64;
    if transfer.received.saturating_add(chunk_len) > transfer.expected_size {
      return Err(TransferError::SizeOverflow {
        id,
        expected_size: transfer.expected_size,
        received: transfer.received,
        chunk_len,
      });
    }

    let mut file = OpenOptions::new().append(true).open(&transfer.partial_path).await?;
    file.write_all(&bytes).await?;
    drop(file);
    transfer.received += chunk_len;
    transfer.last_touched_unix = unix_now();
    self.total_disk_bytes = self.total_disk_bytes.saturating_add(chunk_len);
    write_meta(&transfer.meta_path, &meta_from(transfer)).await?;

    if !last {
      return Ok(ChunkOutcome::Continue {
        received: transfer.received,
      });
    }

    if transfer.received != transfer.expected_size {
      return Err(TransferError::SizeMismatch {
        id,
        expected_size: transfer.expected_size,
        received: transfer.received,
      });
    }

    let actual_sha = hash_file(&transfer.partial_path).await?;
    if let Some(expected) = transfer.expected_sha256.as_deref()
      && !actual_sha.eq_ignore_ascii_case(expected)
    {
      return Err(TransferError::HashMismatch {
        id,
        expected: expected.to_string(),
        actual: actual_sha,
      });
    }

    let transfer = self.transfers.remove(&id).expect("present above");
    self.total_disk_bytes = self.total_disk_bytes.saturating_sub(transfer.received);
    let _ = tokio::fs::remove_file(&transfer.meta_path).await;
    Ok(ChunkOutcome::Completed {
      path: transfer.partial_path,
      sha256: actual_sha,
    })
  }

  async fn handle_abandon(&mut self, id: String) -> Result<(), TransferError> {
    if let Some(transfer) = self.transfers.remove(&id) {
      self.total_disk_bytes = self.total_disk_bytes.saturating_sub(transfer.received);
      let _ = tokio::fs::remove_file(&transfer.partial_path).await;
      let _ = tokio::fs::remove_file(&transfer.meta_path).await;
    }
    Ok(())
  }

  /// Evict oldest-first until projected total fits the budget. Used
  /// when a Begin would push the daemon over budget; the new transfer
  /// has not yet been registered when this runs, so the eviction set
  /// is exactly "everything currently in flight."
  async fn evict_until_under(&mut self, incoming_size: u64) {
    while self.total_disk_bytes.saturating_add(incoming_size) > TRANSFER_DISK_BUDGET_BYTES {
      let Some(victim_id) = self
        .transfers
        .iter()
        .min_by_key(|(_, t)| t.last_touched_unix)
        .map(|(id, _)| id.clone())
      else {
        break;
      };
      tracing::warn!(
        id = %victim_id,
        bytes = self.total_disk_bytes,
        "transfer: evicting oldest in-flight to free disk budget"
      );
      let _ = self.handle_abandon(victim_id).await;
    }
  }

  async fn sweep_stale(&mut self) {
    let now = unix_now();
    let stale: Vec<String> = self
      .transfers
      .iter()
      .filter(|(_, t)| (now - t.last_touched_unix) as u64 >= STALE_TIMEOUT.as_secs())
      .map(|(id, _)| id.clone())
      .collect();
    for id in stale {
      tracing::info!(%id, "transfer: GCing stale partial");
      let _ = self.handle_abandon(id).await;
    }
  }
}

async fn load_recovered_transfer(meta_path: &Path) -> Result<Option<Transfer>, TransferError> {
  let raw = tokio::fs::read(meta_path).await?;
  let meta: Meta = serde_json::from_slice(&raw)?;

  let stem = safe_filename(&meta.id);
  let expected_partial = meta_path.with_file_name(format!("{stem}.partial"));
  if !expected_partial.exists() {
    return Ok(None);
  }

  let actual_size = tokio::fs::metadata(&expected_partial).await?.len();
  if actual_size > meta.received {
    let f = OpenOptions::new().write(true).open(&expected_partial).await?;
    f.set_len(meta.received).await?;
  }
  if actual_size < meta.received {
    let f = OpenOptions::new().write(true).open(&expected_partial).await?;
    f.set_len(actual_size).await?;
  }
  let received = std::cmp::min(actual_size, meta.received);

  Ok(Some(Transfer {
    id: meta.id,
    expected_size: meta.expected_size,
    expected_sha256: meta.expected_sha256,
    received,
    last_touched_unix: meta.last_touched_unix,
    partial_path: expected_partial,
    meta_path: meta_path.to_path_buf(),
  }))
}

async fn sweep_orphan_partials(transfers_dir: &Path, transfers: &HashMap<String, Transfer>) {
  let known_partials: std::collections::HashSet<PathBuf> = transfers.values().map(|t| t.partial_path.clone()).collect();
  let Ok(mut entries) = tokio::fs::read_dir(transfers_dir).await else {
    return;
  };
  while let Ok(Some(entry)) = entries.next_entry().await {
    let path = entry.path();
    if path.extension().and_then(|s| s.to_str()) != Some("partial") {
      continue;
    }
    if known_partials.contains(&path) {
      continue;
    }
    let stale = match tokio::fs::metadata(&path).await {
      Ok(m) => m
        .modified()
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|d| d > STALE_TIMEOUT)
        .unwrap_or(true),
      Err(_) => true,
    };
    if stale {
      tracing::info!(path = %path.display(), "transfer: removing orphan partial on bootstrap");
      let _ = tokio::fs::remove_file(&path).await;
    }
  }
}

async fn write_meta(meta_path: &Path, meta: &Meta) -> Result<(), TransferError> {
  let bytes = serde_json::to_vec(meta)?;
  tokio::fs::write(meta_path, bytes).await?;
  Ok(())
}

async fn hash_file(path: &Path) -> Result<String, TransferError> {
  let mut file = File::open(path).await?;
  file.seek(std::io::SeekFrom::Start(0)).await?;
  let mut hasher = Sha256::new();
  let mut buf = vec![0u8; 64 * 1024];
  loop {
    let n = file.read(&mut buf).await?;
    if n == 0 {
      break;
    }
    hasher.update(&buf[..n]);
  }
  Ok(hex::encode(hasher.finalize()))
}

fn meta_from(t: &Transfer) -> Meta {
  Meta {
    id: t.id.clone(),
    expected_size: t.expected_size,
    expected_sha256: t.expected_sha256.clone(),
    received: t.received,
    last_touched_unix: t.last_touched_unix,
  }
}

fn unix_now() -> i64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|d| d.as_secs() as i64)
    .unwrap_or(0)
}
