//! Asset push pipeline. Owns the post-receipt routing for asset
//! pushes: validates the single-frame Push caps, drives ChunkedTransfer
//! and AssetCache for the chunked Begin/Chunk/Abandon path, and holds
//! the per-id `(mime, retention)` state that bridges `PushBegin` to
//! the matching `PushChunk(last:true)`.
//!
//! The gateway dispatcher posts every asset event/command to this
//! actor's mailbox. Chunk ordering is preserved by single-sender FIFO
//! into the actor; disk work runs on the actor's task, never on the
//! gateway thread.

use std::collections::HashMap;

use libbridgething::{AssetRetention, gateway::ASSET_PUSH_SINGLE_FRAME_MAX_BYTES};
use tokio::{
  sync::{mpsc, oneshot},
  task::JoinHandle,
};
use tokio_util::bytes::Bytes;

use super::AssetCache;
use crate::transfer::{ChunkOutcome, ChunkedTransfer, TransferError};

const COMMAND_MAILBOX_CAPACITY: usize = 16;

#[derive(Debug)]
enum Command {
  Push {
    id: String,
    bytes: Bytes,
    mime: Option<String>,
    retention: AssetRetention,
  },
  Begin {
    id: String,
    expected_size: u64,
    expected_sha256: Option<String>,
    mime: Option<String>,
    retention: AssetRetention,
    ack: oneshot::Sender<Result<u64, TransferError>>,
  },
  Chunk {
    id: String,
    offset: u64,
    bytes: Bytes,
    last: bool,
  },
  Abandon {
    id: String,
  },
  Clear {
    id: String,
  },
}

#[derive(Debug, Clone)]
pub struct AssetIngest {
  cmd_tx: mpsc::Sender<Command>,
}

impl AssetIngest {
  pub fn spawn(transfers: ChunkedTransfer, assets: AssetCache) -> (Self, JoinHandle<()>) {
    let (cmd_tx, cmd_rx) = mpsc::channel(COMMAND_MAILBOX_CAPACITY);
    let actor = AssetIngestActor {
      transfers,
      assets,
      pending: HashMap::new(),
      cmd_rx,
    };
    let handle = tokio::spawn(actor.run());
    (Self { cmd_tx }, handle)
  }

  pub async fn push(&self, id: String, bytes: Bytes, mime: Option<String>, retention: AssetRetention) {
    if self
      .cmd_tx
      .send(Command::Push {
        id,
        bytes,
        mime,
        retention,
      })
      .await
      .is_err()
    {
      tracing::error!("asset ingest mailbox closed; dropping Push");
    }
  }

  pub async fn begin(
    &self,
    id: String,
    expected_size: u64,
    expected_sha256: Option<String>,
    mime: Option<String>,
    retention: AssetRetention,
  ) -> Result<u64, TransferError> {
    let (ack, rx) = oneshot::channel();
    self
      .cmd_tx
      .send(Command::Begin {
        id,
        expected_size,
        expected_sha256,
        mime,
        retention,
        ack,
      })
      .await
      .map_err(|_| TransferError::ActorClosed)?;
    rx.await.map_err(|_| TransferError::ActorClosed)?
  }

  pub async fn chunk(&self, id: String, offset: u64, bytes: Bytes, last: bool) {
    if self
      .cmd_tx
      .send(Command::Chunk {
        id,
        offset,
        bytes,
        last,
      })
      .await
      .is_err()
    {
      tracing::error!("asset ingest mailbox closed; dropping Chunk");
    }
  }

  pub async fn abandon(&self, id: String) {
    if self.cmd_tx.send(Command::Abandon { id }).await.is_err() {
      tracing::error!("asset ingest mailbox closed; dropping Abandon");
    }
  }

  pub async fn clear(&self, id: String) {
    if self.cmd_tx.send(Command::Clear { id }).await.is_err() {
      tracing::error!("asset ingest mailbox closed; dropping Clear");
    }
  }
}

struct PendingPush {
  mime: Option<String>,
  retention: AssetRetention,
}

struct AssetIngestActor {
  transfers: ChunkedTransfer,
  assets: AssetCache,
  pending: HashMap<String, PendingPush>,
  cmd_rx: mpsc::Receiver<Command>,
}

impl AssetIngestActor {
  async fn run(mut self) {
    tracing::debug!("asset ingest actor running");
    while let Some(cmd) = self.cmd_rx.recv().await {
      self.handle(cmd).await;
    }
    tracing::debug!("asset ingest actor exiting");
  }

  async fn handle(&mut self, cmd: Command) {
    match cmd {
      Command::Push {
        id,
        bytes,
        mime,
        retention,
      } => self.handle_push(id, bytes, mime, retention).await,
      Command::Begin {
        id,
        expected_size,
        expected_sha256,
        mime,
        retention,
        ack,
      } => {
        self
          .handle_begin(id, expected_size, expected_sha256, mime, retention, ack)
          .await
      }
      Command::Chunk {
        id,
        offset,
        bytes,
        last,
      } => self.handle_chunk(id, offset, bytes, last).await,
      Command::Abandon { id } => self.handle_abandon(id).await,
      Command::Clear { id } => self.handle_clear(id).await,
    }
  }

  async fn handle_push(&self, id: String, bytes: Bytes, mime: Option<String>, retention: AssetRetention) {
    if bytes.len() > ASSET_PUSH_SINGLE_FRAME_MAX_BYTES {
      tracing::warn!(
        id = %id,
        size = bytes.len(),
        cap = ASSET_PUSH_SINGLE_FRAME_MAX_BYTES,
        "rejecting single-frame Push exceeding {} byte cap; companion must use chunked PushBegin",
        ASSET_PUSH_SINGLE_FRAME_MAX_BYTES,
      );
      return;
    }
    if matches!(retention, AssetRetention::Persistent) {
      tracing::warn!(
        id = %id,
        "rejecting single-frame Push with Persistent retention; companion must use chunked PushBegin",
      );
      return;
    }
    if let Err(err) = self.assets.insert(id, bytes, mime, retention).await {
      tracing::error!(?err, "asset cache insert failed");
    }
  }

  async fn handle_begin(
    &mut self,
    id: String,
    expected_size: u64,
    expected_sha256: Option<String>,
    mime: Option<String>,
    retention: AssetRetention,
    ack: oneshot::Sender<Result<u64, TransferError>>,
  ) {
    match self.transfers.begin(id.clone(), expected_size, expected_sha256).await {
      Ok(offset) => {
        self.pending.insert(id, PendingPush { mime, retention });
        let _ = ack.send(Ok(offset));
      }
      Err(err) => {
        let _ = ack.send(Err(err));
      }
    }
  }

  async fn handle_chunk(&mut self, id: String, offset: u64, bytes: Bytes, last: bool) {
    let outcome = self.transfers.accept_chunk(id.clone(), offset, bytes, last).await;
    match outcome {
      Ok(ChunkOutcome::Continue { .. }) => {}
      Ok(ChunkOutcome::Completed { path, .. }) => {
        let Some(pending) = self.pending.remove(&id) else {
          tracing::warn!(id = %id, "AssetPushChunk completed but no PendingPush state; dropping partial");
          let _ = tokio::fs::remove_file(&path).await;
          return;
        };
        if let Err(err) = self
          .assets
          .insert_from_path(id.clone(), path, pending.mime, pending.retention)
          .await
        {
          tracing::error!(?err, id = %id, "asset cache insert_from_path failed");
        }
      }
      Err(err) => {
        tracing::warn!(?err, id = %id, "AssetPushChunk: chunk rejected by transfer");
        self.pending.remove(&id);
        let _ = self.transfers.abandon(id).await;
      }
    }
  }

  async fn handle_abandon(&mut self, id: String) {
    self.pending.remove(&id);
    if let Err(err) = self.transfers.abandon(id).await {
      tracing::warn!(?err, "transfer abandon failed");
    }
  }

  async fn handle_clear(&self, id: String) {
    if let Err(err) = self.assets.clear(&id).await {
      tracing::error!(?err, "asset cache clear failed");
    }
  }
}
