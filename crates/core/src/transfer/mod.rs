//! Disk-backed chunked transfer primitive. The single intermediate
//! buffer for any wire surface that uses a `Begin` + `Chunk` shape -
//! today that's OTA artifact streaming.
//!
//! Each transfer lives as two files under `transfers_dir`: a `.partial`
//! holding the bytes received so far and a `.meta` json sidecar
//! recording `(id, expected_size, expected_sha256?, received,
//! last_touched_unix)`. Filenames are `hex(sha256(id))` so arbitrary
//! ids (including ones with `/` from the asset namespace) are
//! filesystem-safe without escaping.
//!
//! Bytes never accumulate in memory: each chunk is appended to the
//! partial file as it arrives, the meta sidecar is rewritten, and the
//! Bytes is dropped. Peak daemon-side memory cost is one chunk in
//! flight regardless of total transfer size.
//!
//! Resume across daemon restarts: on bootstrap, every meta sidecar is
//! loaded; partial files longer than `meta.received` (post-power-loss
//! state where bytes hit the file but the meta write didn't) are
//! truncated. The companion's next `begin` for that id sees the
//! recovered offset and continues.

mod actor;
pub mod outbound;
pub mod sinks;

use std::{path::PathBuf, sync::Arc, time::Duration};

use actor::{ChunkedTransferActor, Command};
use sha2::{Digest, Sha256};
use tokio::{
  sync::{mpsc, oneshot},
  task::JoinHandle,
};
use tokio_util::bytes::Bytes;

pub const TRANSFER_DISK_BUDGET_BYTES: u64 = 1024 * 1024 * 1024;
const STALE_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);
const SWEEP_INTERVAL: Duration = Duration::from_secs(60 * 60);
const COMMAND_MAILBOX_CAPACITY: usize = 16;

#[derive(Debug)]
pub enum ChunkOutcome {
  Continue { received: u64 },
  Completed { path: PathBuf, sha256: String },
}

#[derive(Debug, Clone)]
pub struct ChunkedTransfer {
  inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
  cmd_tx: mpsc::Sender<Command>,
}

impl ChunkedTransfer {
  pub async fn init(transfers_dir: PathBuf) -> Result<ChunkedTransferPending, TransferError> {
    tokio::fs::create_dir_all(&transfers_dir).await?;
    let (cmd_tx, cmd_rx) = mpsc::channel(COMMAND_MAILBOX_CAPACITY);
    let actor = ChunkedTransferActor::bootstrap(transfers_dir, cmd_rx, cmd_tx.clone()).await?;
    Ok(ChunkedTransferPending {
      actor,
      handle: Self {
        inner: Arc::new(Inner { cmd_tx }),
      },
    })
  }

  pub async fn begin(
    &self,
    id: String,
    expected_size: u64,
    expected_sha256: Option<String>,
    target_dir: Option<PathBuf>,
  ) -> Result<u64, TransferError> {
    let (ack, rx) = oneshot::channel();
    self
      .inner
      .cmd_tx
      .send(Command::Begin {
        id,
        expected_size,
        expected_sha256,
        target_dir,
        ack,
      })
      .await
      .map_err(|_| TransferError::ActorClosed)?;
    rx.await.map_err(|_| TransferError::ActorClosed)?
  }

  pub async fn accept_chunk(
    &self,
    id: String,
    offset: u64,
    bytes: Bytes,
    last: bool,
  ) -> Result<ChunkOutcome, TransferError> {
    let (ack, rx) = oneshot::channel();
    self
      .inner
      .cmd_tx
      .send(Command::AcceptChunk {
        id,
        offset,
        bytes,
        last,
        ack,
      })
      .await
      .map_err(|_| TransferError::ActorClosed)?;
    rx.await.map_err(|_| TransferError::ActorClosed)?
  }

  pub async fn abandon(&self, id: String) -> Result<(), TransferError> {
    let (ack, rx) = oneshot::channel();
    self
      .inner
      .cmd_tx
      .send(Command::Abandon { id, ack })
      .await
      .map_err(|_| TransferError::ActorClosed)?;
    rx.await.map_err(|_| TransferError::ActorClosed)?
  }
}

pub struct ChunkedTransferPending {
  actor: ChunkedTransferActor,
  handle: ChunkedTransfer,
}

impl ChunkedTransferPending {
  pub fn spawn(self) -> (ChunkedTransfer, JoinHandle<()>) {
    let join = tokio::spawn(self.actor.run());
    (self.handle, join)
  }
}

fn safe_filename(id: &str) -> String {
  let mut h = Sha256::new();
  h.update(id.as_bytes());
  hex::encode(h.finalize())
}

#[derive(Debug, thiserror::Error)]
pub enum TransferError {
  #[error("transfer actor channel closed")]
  ActorClosed,
  #[error("io error: {0}")]
  Io(#[from] std::io::Error),
  #[error("meta sidecar serialize error: {0}")]
  MetaEncode(#[from] serde_json::Error),
  #[error("transfer {id} would exceed disk budget ({} GiB)", TRANSFER_DISK_BUDGET_BYTES / (1024 * 1024 * 1024))]
  BudgetExceeded { id: String },
  #[error("transfer {id} expected_size {size} exceeds disk budget")]
  TooLarge { id: String, size: u64 },
  #[error("transfer {id} already in flight with mismatched expected_size or sha256; abandon first")]
  ConflictingBegin { id: String },
  #[error("transfer {id} unknown; chunk arrived without matching begin")]
  UnknownTransfer { id: String },
  #[error("transfer {id} chunk offset {got} != expected {expected}")]
  OffsetMismatch { id: String, expected: u64, got: u64 },
  #[error("transfer {id} chunk would push past expected_size {expected_size} (received {received}, chunk {chunk_len})")]
  SizeOverflow {
    id: String,
    expected_size: u64,
    received: u64,
    chunk_len: u64,
  },
  #[error("transfer {id} last chunk arrived but received {received} != expected_size {expected_size}")]
  SizeMismatch {
    id: String,
    expected_size: u64,
    received: u64,
  },
  #[error("transfer {id} sha256 mismatch: expected {expected}, got {actual}")]
  HashMismatch {
    id: String,
    expected: String,
    actual: String,
  },
}

#[cfg(test)]
mod tests {
  use sha2::{Digest, Sha256};

  use super::*;

  fn temp_root() -> PathBuf {
    let p = std::env::temp_dir().join(format!("bridgething-transfer-test-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&p).unwrap();
    p
  }

  async fn fresh() -> (ChunkedTransfer, PathBuf, JoinHandle<()>) {
    let root = temp_root();
    let pending = ChunkedTransfer::init(root.clone()).await.unwrap();
    let (handle, join) = pending.spawn();
    (handle, root, join)
  }

  fn sha256_hex(b: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b);
    hex::encode(h.finalize())
  }

  #[tokio::test]
  async fn happy_path_one_chunk_completes() {
    let (xfer, _root, _join) = fresh().await;
    let body = b"hello world".to_vec();
    let sha = sha256_hex(&body);

    let off = xfer
      .begin("t/1".into(), body.len() as u64, Some(sha.clone()), None)
      .await
      .unwrap();
    assert_eq!(off, 0);

    let outcome = xfer
      .accept_chunk("t/1".into(), 0, Bytes::from(body.clone()), true)
      .await
      .unwrap();
    match outcome {
      ChunkOutcome::Completed { path, sha256 } => {
        assert_eq!(sha256, sha);
        let bytes = tokio::fs::read(&path).await.unwrap();
        assert_eq!(bytes, body);
      }
      _ => panic!("expected Completed"),
    }
  }

  #[tokio::test]
  async fn multi_chunk_completes() {
    let (xfer, _root, _join) = fresh().await;
    let body: Vec<u8> = (0u8..=200).cycle().take(4096).collect();
    let sha = sha256_hex(&body);

    let off = xfer
      .begin("t/2".into(), body.len() as u64, Some(sha.clone()), None)
      .await
      .unwrap();
    assert_eq!(off, 0);

    let chunk_size = 1024;
    let mut offset = 0u64;
    while offset < body.len() as u64 {
      let end = (offset + chunk_size).min(body.len() as u64) as usize;
      let last = end as u64 == body.len() as u64;
      let chunk = Bytes::copy_from_slice(&body[offset as usize..end]);
      let outcome = xfer.accept_chunk("t/2".into(), offset, chunk, last).await.unwrap();
      offset = end as u64;
      match (outcome, last) {
        (ChunkOutcome::Continue { received }, false) => assert_eq!(received, offset),
        (ChunkOutcome::Completed { sha256, .. }, true) => assert_eq!(sha256, sha),
        _ => panic!("unexpected outcome"),
      }
    }
  }

  #[tokio::test]
  async fn offset_mismatch_rejected() {
    let (xfer, _root, _join) = fresh().await;
    let body = [0u8; 100];
    xfer.begin("t/3".into(), body.len() as u64, None, None).await.unwrap();
    xfer
      .accept_chunk("t/3".into(), 0, Bytes::copy_from_slice(&body[..50]), false)
      .await
      .unwrap();
    let err = xfer
      .accept_chunk("t/3".into(), 99, Bytes::copy_from_slice(&body[..50]), true)
      .await
      .unwrap_err();
    assert!(matches!(
      err,
      TransferError::OffsetMismatch {
        expected: 50,
        got: 99,
        ..
      }
    ));
  }

  #[tokio::test]
  async fn size_overflow_rejected() {
    let (xfer, _root, _join) = fresh().await;
    xfer.begin("t/4".into(), 100, None, None).await.unwrap();
    let err = xfer
      .accept_chunk("t/4".into(), 0, Bytes::from(vec![0u8; 200]), true)
      .await
      .unwrap_err();
    assert!(matches!(err, TransferError::SizeOverflow { .. }));
  }

  #[tokio::test]
  async fn hash_mismatch_rejects_completion() {
    let (xfer, _root, _join) = fresh().await;
    let body = b"hello".to_vec();
    xfer
      .begin("t/5".into(), body.len() as u64, Some("0".repeat(64)), None)
      .await
      .unwrap();
    let err = xfer
      .accept_chunk("t/5".into(), 0, Bytes::from(body), true)
      .await
      .unwrap_err();
    assert!(matches!(err, TransferError::HashMismatch { .. }));
  }

  #[tokio::test]
  async fn resume_picks_up_where_left_off() {
    let root = temp_root();
    let pending = ChunkedTransfer::init(root.clone()).await.unwrap();
    let (xfer, _join) = pending.spawn();

    let body: Vec<u8> = (0u8..=255).cycle().take(2048).collect();
    let sha = sha256_hex(&body);
    xfer
      .begin("t/6".into(), body.len() as u64, Some(sha.clone()), None)
      .await
      .unwrap();
    xfer
      .accept_chunk("t/6".into(), 0, Bytes::copy_from_slice(&body[..1024]), false)
      .await
      .unwrap();
    drop(xfer);

    let pending2 = ChunkedTransfer::init(root.clone()).await.unwrap();
    let (xfer2, _join2) = pending2.spawn();
    let off = xfer2
      .begin("t/6".into(), body.len() as u64, Some(sha.clone()), None)
      .await
      .unwrap();
    assert_eq!(off, 1024);

    let outcome = xfer2
      .accept_chunk("t/6".into(), 1024, Bytes::copy_from_slice(&body[1024..]), true)
      .await
      .unwrap();
    match outcome {
      ChunkOutcome::Completed { sha256, .. } => assert_eq!(sha256, sha),
      _ => panic!("expected Completed"),
    }
  }

  #[tokio::test]
  async fn conflicting_begin_rejected() {
    let (xfer, _root, _join) = fresh().await;
    xfer.begin("t/7".into(), 100, None, None).await.unwrap();
    let err = xfer.begin("t/7".into(), 200, None, None).await.unwrap_err();
    assert!(matches!(err, TransferError::ConflictingBegin { .. }));
  }

  #[tokio::test]
  async fn abandon_clears_partial_and_meta() {
    let (xfer, root, _join) = fresh().await;
    xfer.begin("t/8".into(), 100, None, None).await.unwrap();
    xfer
      .accept_chunk("t/8".into(), 0, Bytes::from(vec![0u8; 50]), false)
      .await
      .unwrap();
    xfer.abandon("t/8".into()).await.unwrap();
    let stem = safe_filename("t/8");
    assert!(!root.join(format!("{stem}.partial")).exists());
    assert!(!root.join(format!("{stem}.meta")).exists());
  }

  #[tokio::test]
  async fn unknown_chunk_rejected() {
    let (xfer, _root, _join) = fresh().await;
    let err = xfer
      .accept_chunk("ghost".into(), 0, Bytes::from(vec![0u8; 1]), false)
      .await
      .unwrap_err();
    assert!(matches!(err, TransferError::UnknownTransfer { .. }));
  }

  #[tokio::test]
  async fn over_budget_begin_rejected() {
    let (xfer, _root, _join) = fresh().await;
    let err = xfer
      .begin("t/9".into(), TRANSFER_DISK_BUDGET_BYTES + 1, None, None)
      .await
      .unwrap_err();
    assert!(matches!(err, TransferError::TooLarge { .. }));
  }
}
