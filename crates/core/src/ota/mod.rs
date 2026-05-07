//! OTA orchestrator. Drives a `.swu` from "first chunk arriving on
//! the wire" to "device rebooted onto the new slot", emitting
//! `OtaProgress` events at every phase transition and an `OtaError`
//! on terminal failure.
//!
//! Phase machine:
//!
//!     Idle --[OtaBegin]--> Streaming --[last chunk]--> Verifying
//!                                                       |
//!                                       [hash/size ok]  |
//!                                                       v
//!                                                    Writing --[ok]--> Confirming --> Reboot
//!
//! Cancellation is honored through `Writing`; once we hit `Confirming`
//! the slot flip is committed and we don't roll back. `CancelUpdate`
//! keeps the partial on disk for a future resume; `OtaAbandon` is the
//! explicit clean-up path.
//!
//! Single-instance: a fresh `OtaBegin` arriving while an OTA is
//! actively writing rejects with `OtaBeginRejected`. A new `OtaBegin`
//! for a different update_id while one is in `Streaming` cancels the
//! prior streaming run (the partial stays for resume) and starts the
//! new one.
//!
//! Bytes never accumulate in memory: chunks land on
//! `<runtime_dir>/transfers/<id>.partial` via `ChunkedTransfer`, and
//! libswupdate consumes from that on-disk file at write time.

mod slots;
mod swupdate;

use std::{path::PathBuf, sync::Arc};

use libbridgething::{
  OtaError, OtaErrorCode, OtaPhase, OtaProgress,
  gateway::{BridgeToGatewaySystemMsgEvent, OtaBegin, OtaBeginAck, OtaBeginRejected, OtaChunk},
};
use tokio::{
  sync::{mpsc, oneshot, watch},
  task::JoinHandle,
};
use tokio_util::bytes::Bytes;

use crate::transfer::{ChunkOutcome, ChunkedTransfer, TransferError};

pub type OtaEventTx = mpsc::Sender<BridgeToGatewaySystemMsgEvent>;

#[derive(Debug)]
enum Command {
  Begin {
    req: OtaBegin,
    ack: oneshot::Sender<Result<OtaBeginAck, OtaBeginRejected>>,
  },
  Chunk(OtaChunk),
  Abandon {
    update_id: String,
  },
  Cancel,
  WriteFinished,
}

/// Thunk the orchestrator calls when entering the terminal `Reboot`
/// phase. Production wires this to systemd's `Reboot` D-Bus method;
/// tests can pass a no-op.
pub type RebootFn = Arc<dyn Fn() + Send + Sync + 'static>;

#[derive(Clone)]
pub struct OtaOrchestrator {
  cmd_tx: mpsc::Sender<Command>,
}

impl std::fmt::Debug for OtaOrchestrator {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("OtaOrchestrator").finish_non_exhaustive()
  }
}

impl OtaOrchestrator {
  pub fn spawn(transfers: ChunkedTransfer, events_tx: OtaEventTx, reboot: RebootFn) -> (Self, JoinHandle<()>) {
    let (cmd_tx, cmd_rx) = mpsc::channel(64);
    let actor = OtaActor {
      transfers,
      events_tx,
      reboot,
      self_tx: cmd_tx.clone(),
      cmd_rx,
      state: OtaState::Idle,
    };
    let handle = tokio::spawn(actor.run());
    (Self { cmd_tx }, handle)
  }

  pub async fn begin(&self, req: OtaBegin) -> Result<OtaBeginAck, OtaBeginRejected> {
    let (ack, rx) = oneshot::channel();
    if self.cmd_tx.send(Command::Begin { req, ack }).await.is_err() {
      return Err(OtaBeginRejected {
        reason: "ota orchestrator mailbox closed".into(),
      });
    }
    rx.await.unwrap_or_else(|_| {
      Err(OtaBeginRejected {
        reason: "ota orchestrator dropped reply".into(),
      })
    })
  }

  pub async fn chunk(&self, chunk: OtaChunk) {
    if let Err(err) = self.cmd_tx.send(Command::Chunk(chunk)).await {
      tracing::error!(?err, "ota orchestrator mailbox closed; dropping OtaChunk");
    }
  }

  pub async fn abandon(&self, update_id: String) {
    if let Err(err) = self.cmd_tx.send(Command::Abandon { update_id }).await {
      tracing::error!(?err, "ota orchestrator mailbox closed; dropping OtaAbandon");
    }
  }

  pub async fn cancel(&self) {
    if let Err(err) = self.cmd_tx.send(Command::Cancel).await {
      tracing::error!(?err, "ota orchestrator mailbox closed; dropping CancelUpdate");
    }
  }

  pub async fn asset_range_chunk(&self, chunk: libbridgething::gateway::OtaAssetRangeChunk) {
    tracing::warn!(
      request_id = %chunk.request_id,
      part = chunk.part_index,
      offset = chunk.offset,
      len = chunk.bytes.len(),
      "OtaAssetRangeChunk arrived but range proxy is not yet wired; dropping",
    );
  }
}

enum OtaState {
  /// No OTA in flight. Begin opens a transfer, transitions to Streaming.
  Idle,
  /// Companion is pushing chunks. Last chunk transitions to Writing
  /// (via the post-stream Verifying phase, which collapses to a single
  /// progress event since the chunk loop already verified per-chunk
  /// against expected_size + offset; we re-hash at completion).
  Streaming { update_id: String, expected_size: u64 },
  /// libswupdate is consuming the file. Cancel signaller is live;
  /// new OtaBegin is rejected.
  Writing {
    update_id: String,
    cancel_tx: watch::Sender<bool>,
  },
}

struct OtaActor {
  transfers: ChunkedTransfer,
  events_tx: OtaEventTx,
  reboot: RebootFn,
  /// Self-send channel for the spawned write task to post `WriteFinished`
  /// so `state` is cleared back through the actor without sharing state.
  self_tx: mpsc::Sender<Command>,
  cmd_rx: mpsc::Receiver<Command>,
  state: OtaState,
}

impl OtaActor {
  async fn run(mut self) {
    tracing::info!("ota orchestrator started");
    while let Some(cmd) = self.cmd_rx.recv().await {
      match cmd {
        Command::Begin { req, ack } => self.handle_begin(req, ack).await,
        Command::Chunk(chunk) => self.handle_chunk(chunk).await,
        Command::Abandon { update_id } => self.handle_abandon(update_id).await,
        Command::Cancel => self.handle_cancel().await,
        Command::WriteFinished => {
          self.state = OtaState::Idle;
        }
      }
    }
    tracing::info!("ota orchestrator exiting");
  }

  async fn handle_begin(&mut self, req: OtaBegin, ack: oneshot::Sender<Result<OtaBeginAck, OtaBeginRejected>>) {
    if let OtaState::Writing { update_id, .. } = &self.state {
      let _ = ack.send(Err(OtaBeginRejected {
        reason: format!("ota write of {update_id} in progress; cancel or wait first"),
      }));
      return;
    }

    if let OtaState::Streaming {
      update_id: existing, ..
    } = &self.state
      && existing != &req.update_id
    {
      tracing::info!(
        prior = %existing,
        new = %req.update_id,
        "OtaBegin for new update_id during streaming; abandoning prior partial",
      );
      let _ = self.transfers.abandon(existing.clone()).await;
      self.state = OtaState::Idle;
    }

    let result = self
      .transfers
      .begin(
        req.update_id.clone(),
        req.expected_size as u64,
        Some(req.expected_sha256.clone()),
      )
      .await;
    match result {
      Ok(resume_from_offset) => {
        emit_progress(
          &self.events_tx,
          OtaPhase::Streaming,
          phase_percent(resume_from_offset, req.expected_size as u64),
          None,
        )
        .await;
        self.state = OtaState::Streaming {
          update_id: req.update_id,
          expected_size: req.expected_size as u64,
        };
        let _ = ack.send(Ok(OtaBeginAck {
          resume_from_offset: resume_from_offset as u32,
        }));
      }
      Err(err) => {
        let _ = ack.send(Err(OtaBeginRejected {
          reason: format!("transfer begin failed: {err}"),
        }));
      }
    }
  }

  async fn handle_chunk(&mut self, chunk: OtaChunk) {
    let (current_id, expected_size) = match &self.state {
      OtaState::Streaming {
        update_id,
        expected_size,
      } => (update_id.clone(), *expected_size),
      _ => {
        tracing::warn!(
          update_id = %chunk.update_id,
          "OtaChunk arrived outside Streaming state; emitting UnknownUpdate",
        );
        emit_error(
          &self.events_tx,
          OtaErrorCode::UnknownUpdate,
          format!("no active OTA for {}", chunk.update_id),
        )
        .await;
        return;
      }
    };
    if current_id != chunk.update_id {
      emit_error(
        &self.events_tx,
        OtaErrorCode::UnknownUpdate,
        format!("expected chunks for {current_id}, got {}", chunk.update_id),
      )
      .await;
      return;
    }

    let outcome = self
      .transfers
      .accept_chunk(
        chunk.update_id.clone(),
        chunk.offset as u64,
        Bytes::from(chunk.bytes),
        chunk.last,
      )
      .await;
    match outcome {
      Ok(ChunkOutcome::Continue { received }) => {
        emit_progress(
          &self.events_tx,
          OtaPhase::Streaming,
          phase_percent(received, expected_size),
          None,
        )
        .await;
      }
      Ok(ChunkOutcome::Completed { path, .. }) => {
        emit_progress(&self.events_tx, OtaPhase::Streaming, 100, None).await;
        emit_progress(&self.events_tx, OtaPhase::Verifying, 100, None).await;
        self.spawn_write(current_id, path).await;
      }
      Err(err) => {
        let code = transfer_error_code(&err);
        emit_error(&self.events_tx, code, format!("ota chunk: {err}")).await;
        let _ = self.transfers.abandon(current_id).await;
        self.state = OtaState::Idle;
      }
    }
  }

  async fn handle_abandon(&mut self, update_id: String) {
    let _ = self.transfers.abandon(update_id.clone()).await;
    if let OtaState::Streaming {
      update_id: streaming, ..
    } = &self.state
      && streaming == &update_id
    {
      self.state = OtaState::Idle;
    }
  }

  async fn handle_cancel(&mut self) {
    match &self.state {
      OtaState::Idle => tracing::debug!("cancel requested with no run in flight; ignoring"),
      OtaState::Streaming { update_id, .. } => {
        tracing::info!(%update_id, "cancel during streaming; partial retained for resume");
        self.state = OtaState::Idle;
      }
      OtaState::Writing { cancel_tx, .. } => {
        tracing::info!("cancel during write; signalling in-flight write");
        let _ = cancel_tx.send(true);
      }
    }
  }

  async fn spawn_write(&mut self, update_id: String, swu_path: PathBuf) {
    let (cancel_tx, cancel_rx) = watch::channel(false);
    self.state = OtaState::Writing {
      update_id: update_id.clone(),
      cancel_tx,
    };

    let events_tx = self.events_tx.clone();
    let reboot = self.reboot.clone();
    let self_tx = self.self_tx.clone();

    tokio::spawn(async move {
      let outcome = run_write_and_confirm(&events_tx, &swu_path, cancel_rx).await;
      let _ = tokio::fs::remove_file(&swu_path).await;
      match outcome {
        Ok(()) => {
          tracing::info!(%update_id, "ota write+confirm complete; triggering reboot");
          (reboot)();
        }
        Err(err) => {
          tracing::warn!(?err, "ota write terminated with error");
          emit_error(&events_tx, err.code, err.msg).await;
        }
      }
      let _ = self_tx.send(Command::WriteFinished).await;
    });
  }
}

async fn emit_error(events_tx: &OtaEventTx, code: OtaErrorCode, msg: String) {
  let _ = events_tx
    .send(BridgeToGatewaySystemMsgEvent::OtaError(OtaError { code, msg }))
    .await;
}

#[derive(Debug)]
struct WriteError {
  code: OtaErrorCode,
  msg: String,
}

async fn run_write_and_confirm(
  events_tx: &OtaEventTx,
  swu_path: &std::path::Path,
  mut cancel_rx: watch::Receiver<bool>,
) -> Result<(), WriteError> {
  if check_cancel(&mut cancel_rx) {
    return Err(WriteError {
      code: OtaErrorCode::Cancelled,
      msg: "cancelled before writing".into(),
    });
  }

  let target = slots::inactive_slot().await.map_err(|err| WriteError {
    code: OtaErrorCode::Internal,
    msg: format!("failed to read inactive slot: {err}"),
  })?;
  tracing::info!(?target, "ota target slot resolved");
  let selector = swupdate::Selector {
    software_set: "stable".into(),
    running_mode: target.selector().into(),
  };

  let progress_emitter = {
    let tx = events_tx.clone();
    move |phase: OtaPhase, percent: u8, eta_ms: Option<u32>| {
      let tx = tx.clone();
      tokio::spawn(async move {
        emit_progress(&tx, phase, percent, eta_ms).await;
      });
    }
  };

  swupdate::install_swu(swu_path, &selector, &progress_emitter, &mut cancel_rx)
    .await
    .map_err(|err| WriteError {
      code: match err {
        swupdate::Error::Cancelled => OtaErrorCode::Cancelled,
        swupdate::Error::Io(_) | swupdate::Error::Ipc(_) | swupdate::Error::InstallFailed(_) => {
          OtaErrorCode::WriteFailed
        }
      },
      msg: format!("swupdate failed: {err}"),
    })?;

  emit_progress(events_tx, OtaPhase::Confirming, 0, None).await;
  slots::confirm_target(target).await.map_err(|err| WriteError {
    code: OtaErrorCode::ConfirmFailed,
    msg: format!("failed to confirm target slot {:?}: {err}", target),
  })?;
  emit_progress(events_tx, OtaPhase::Confirming, 100, None).await;

  emit_progress(events_tx, OtaPhase::Reboot, 0, None).await;
  Ok(())
}

fn check_cancel(rx: &mut watch::Receiver<bool>) -> bool {
  *rx.borrow_and_update()
}

async fn emit_progress(events_tx: &OtaEventTx, phase: OtaPhase, percent: u8, eta_ms: Option<u32>) {
  let _ = events_tx
    .send(BridgeToGatewaySystemMsgEvent::OtaProgress(OtaProgress {
      phase,
      percent,
      eta_ms,
    }))
    .await;
}

fn phase_percent(received: u64, expected: u64) -> u8 {
  if expected == 0 {
    return 100;
  }
  ((received.saturating_mul(100)) / expected).min(100) as u8
}

fn transfer_error_code(err: &TransferError) -> OtaErrorCode {
  match err {
    TransferError::OffsetMismatch { .. } => OtaErrorCode::OffsetMismatch,
    TransferError::SizeOverflow { .. } | TransferError::SizeMismatch { .. } => OtaErrorCode::SizeMismatch,
    TransferError::HashMismatch { .. } => OtaErrorCode::HashMismatch,
    TransferError::UnknownTransfer { .. } => OtaErrorCode::UnknownUpdate,
    _ => OtaErrorCode::Internal,
  }
}

#[cfg(test)]
mod tests {
  use std::sync::atomic::{AtomicUsize, Ordering};

  use libbridgething::gateway::{OtaBegin, OtaChunk};
  use sha2::{Digest, Sha256};
  use tokio::time::{Duration, timeout};

  use super::*;

  fn fixture_bytes() -> (Vec<u8>, String, u32) {
    let bytes = b"fake-swu-payload-for-orchestrator-tests".to_vec();
    let sha = {
      let mut h = Sha256::new();
      h.update(&bytes);
      hex::encode(h.finalize())
    };
    let size = bytes.len() as u32;
    (bytes, sha, size)
  }

  fn temp_root() -> PathBuf {
    let p = std::env::temp_dir().join(format!("bridgething-ota-test-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&p).unwrap();
    p
  }

  struct Harness {
    ota: OtaOrchestrator,
    events: mpsc::Receiver<BridgeToGatewaySystemMsgEvent>,
    reboot_calls: Arc<AtomicUsize>,
    _root: PathBuf,
  }

  async fn boot() -> Harness {
    let root = temp_root();
    let pending = ChunkedTransfer::init(root.clone()).await.unwrap();
    let (transfers, _xfer_handle) = pending.spawn();
    let (events_tx, events) = mpsc::channel(64);
    let reboot_calls = Arc::new(AtomicUsize::new(0));
    let calls = reboot_calls.clone();
    let reboot: RebootFn = Arc::new(move || {
      calls.fetch_add(1, Ordering::SeqCst);
    });
    let (ota, _ota_handle) = OtaOrchestrator::spawn(transfers, events_tx, reboot);
    Harness {
      ota,
      events,
      reboot_calls,
      _root: root,
    }
  }

  async fn wait_for(
    events: &mut mpsc::Receiver<BridgeToGatewaySystemMsgEvent>,
    deadline: Duration,
    pred: impl Fn(&BridgeToGatewaySystemMsgEvent) -> bool,
  ) -> BridgeToGatewaySystemMsgEvent {
    timeout(deadline, async {
      loop {
        let ev = events.recv().await.expect("event channel closed");
        if pred(&ev) {
          return ev;
        }
      }
    })
    .await
    .expect("timed out waiting for matching event")
  }

  #[tokio::test]
  async fn happy_path_streams_completes_and_reboots() {
    let mut h = boot().await;
    let (bytes, sha, size) = fixture_bytes();

    let ack = h
      .ota
      .begin(OtaBegin {
        update_id: sha.clone(),
        update_url_base: None,
        expected_sha256: sha.clone(),
        expected_size: size,
      })
      .await
      .expect("begin ok");
    assert_eq!(ack.resume_from_offset, 0);

    h.ota
      .chunk(OtaChunk {
        update_id: sha.clone(),
        offset: 0,
        bytes,
        last: true,
      })
      .await;

    let _ = wait_for(&mut h.events, Duration::from_secs(10), |ev| {
      matches!(
        ev,
        BridgeToGatewaySystemMsgEvent::OtaProgress(p) if matches!(p.phase, OtaPhase::Reboot)
      )
    })
    .await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(h.reboot_calls.load(Ordering::SeqCst), 1);
  }

  #[tokio::test]
  async fn size_mismatch_emits_error() {
    let mut h = boot().await;
    let (bytes, sha, size) = fixture_bytes();
    h.ota
      .begin(OtaBegin {
        update_id: sha.clone(),
        update_url_base: None,
        expected_sha256: sha.clone(),
        expected_size: size + 1,
      })
      .await
      .expect("begin ok");

    h.ota
      .chunk(OtaChunk {
        update_id: sha,
        offset: 0,
        bytes,
        last: true,
      })
      .await;

    let err = wait_for(&mut h.events, Duration::from_secs(2), |ev| {
      matches!(ev, BridgeToGatewaySystemMsgEvent::OtaError(_))
    })
    .await;
    let BridgeToGatewaySystemMsgEvent::OtaError(e) = err else {
      unreachable!()
    };
    assert_eq!(e.code, OtaErrorCode::SizeMismatch);
  }

  #[tokio::test]
  async fn hash_mismatch_emits_error() {
    let mut h = boot().await;
    let (bytes, _sha, size) = fixture_bytes();
    let bogus_sha = "0".repeat(64);
    h.ota
      .begin(OtaBegin {
        update_id: bogus_sha.clone(),
        update_url_base: None,
        expected_sha256: bogus_sha.clone(),
        expected_size: size,
      })
      .await
      .expect("begin ok");
    h.ota
      .chunk(OtaChunk {
        update_id: bogus_sha,
        offset: 0,
        bytes,
        last: true,
      })
      .await;

    let err = wait_for(&mut h.events, Duration::from_secs(2), |ev| {
      matches!(ev, BridgeToGatewaySystemMsgEvent::OtaError(_))
    })
    .await;
    let BridgeToGatewaySystemMsgEvent::OtaError(e) = err else {
      unreachable!()
    };
    assert_eq!(e.code, OtaErrorCode::HashMismatch);
  }

  #[tokio::test]
  async fn resume_returns_received_offset() {
    let mut h = boot().await;
    let (bytes, sha, size) = fixture_bytes();
    h.ota
      .begin(OtaBegin {
        update_id: sha.clone(),
        update_url_base: None,
        expected_sha256: sha.clone(),
        expected_size: size,
      })
      .await
      .expect("first begin ok");
    h.ota
      .chunk(OtaChunk {
        update_id: sha.clone(),
        offset: 0,
        bytes: bytes[..10].to_vec(),
        last: false,
      })
      .await;

    h.ota.cancel().await;
    let ack = h
      .ota
      .begin(OtaBegin {
        update_id: sha.clone(),
        update_url_base: None,
        expected_sha256: sha.clone(),
        expected_size: size,
      })
      .await
      .expect("resume begin ok");
    assert_eq!(ack.resume_from_offset, 10);

    h.ota
      .chunk(OtaChunk {
        update_id: sha.clone(),
        offset: 10,
        bytes: bytes[10..].to_vec(),
        last: true,
      })
      .await;
    let _ = wait_for(&mut h.events, Duration::from_secs(10), |ev| {
      matches!(
        ev,
        BridgeToGatewaySystemMsgEvent::OtaProgress(p) if matches!(p.phase, OtaPhase::Reboot)
      )
    })
    .await;
  }

  #[tokio::test]
  async fn second_begin_during_write_is_rejected() {
    let mut h = boot().await;
    let (bytes, sha, size) = fixture_bytes();
    h.ota
      .begin(OtaBegin {
        update_id: sha.clone(),
        update_url_base: None,
        expected_sha256: sha.clone(),
        expected_size: size,
      })
      .await
      .unwrap();
    h.ota
      .chunk(OtaChunk {
        update_id: sha.clone(),
        offset: 0,
        bytes,
        last: true,
      })
      .await;
    let _ = wait_for(
      &mut h.events,
      Duration::from_secs(5),
      |ev| matches!(ev, BridgeToGatewaySystemMsgEvent::OtaProgress(p) if matches!(p.phase, OtaPhase::Writing)),
    )
    .await;

    let err = h
      .ota
      .begin(OtaBegin {
        update_id: "deadbeef".repeat(8),
        update_url_base: None,
        expected_sha256: "deadbeef".repeat(8),
        expected_size: 32,
      })
      .await
      .unwrap_err();
    assert!(err.reason.contains("in progress"), "got reason: {}", err.reason);
  }

  #[tokio::test]
  async fn abandon_clears_streaming_state() {
    let mut h = boot().await;
    let (bytes, sha, size) = fixture_bytes();
    h.ota
      .begin(OtaBegin {
        update_id: sha.clone(),
        update_url_base: None,
        expected_sha256: sha.clone(),
        expected_size: size,
      })
      .await
      .unwrap();
    h.ota
      .chunk(OtaChunk {
        update_id: sha.clone(),
        offset: 0,
        bytes: bytes[..10].to_vec(),
        last: false,
      })
      .await;
    h.ota.abandon(sha.clone()).await;
    let ack = h
      .ota
      .begin(OtaBegin {
        update_id: sha.clone(),
        update_url_base: None,
        expected_sha256: sha,
        expected_size: size,
      })
      .await
      .expect("begin after abandon");
    assert_eq!(ack.resume_from_offset, 0);
  }
}
