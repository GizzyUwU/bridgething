//! OTA orchestrator. Drives an update from "first chunk arriving on
//! the wire" to "the new bits are live", emitting `OtaProgress` events
//! at every phase transition and an `OtaError` on terminal failure.
//!
//! Two kinds, one phase machine:
//!
//! Image (`.swu`):
//!
//!     Idle --[OtaBegin]--> Streaming --[last chunk]--> Verifying
//!         --> Writing (libswupdate) --> Confirming --> Reboot
//!
//! Daemon (raw aarch64 binary):
//!
//!     Idle --[OtaBegin]--> Streaming --[last chunk]--> Verifying
//!         --> Writing (atomic rename) --> Reboot
//!
//! `Confirming` is image-only (slot try-counter flip). `Reboot` is
//! universal: image fires systemd Reboot, daemon fires `systemctl
//! restart bridgething.service`.
//!
//! Cancellation: image is cancelable through `Writing` (libswupdate
//! honors mid-install cancel). Daemon is cancelable through `Streaming`
//! only - the rename is one syscall with no half-state.
//!
//! Single-instance: a fresh `OtaBegin` arriving while an OTA is
//! actively writing rejects with `OtaBeginRejected`. A new `OtaBegin`
//! for a different update_id while one is in `Streaming` cancels the
//! prior streaming run (the partial stays for resume) and starts the
//! new one.
//!
//! Bytes never accumulate in memory: chunks land on
//! `<state_dir>/transfers/<id>.partial` via `ChunkedTransfer`, and
//! the kind-specific backend consumes from that on-disk file at write
//! time.

mod daemon_swap;
mod range_proxy;
mod slots;
mod swupdate;

use std::{path::PathBuf, sync::Arc};

use bluer::Address;
use libbridgething::{
  OtaError, OtaErrorCode, OtaKind, OtaPhase, OtaProgress,
  gateway::{BridgeToGatewaySystemMsgEvent, OtaAssetRangeChunk, OtaBegin, OtaBeginAck, OtaBeginRejected, OtaChunk},
};
pub use range_proxy::RangeProxy;
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
    peer: Option<Address>,
    ack: oneshot::Sender<Result<OtaBeginAck, OtaBeginRejected>>,
  },
  Chunk(OtaChunk),
  AssetRangeChunk(OtaAssetRangeChunk),
  Abandon {
    update_id: String,
  },
  Cancel,
  WriteFinished,
}

pub type TerminatorFn = Arc<dyn Fn() + Send + Sync + 'static>;

#[derive(Clone)]
pub struct OtaTerminators {
  pub reboot: TerminatorFn,
  pub restart_self: TerminatorFn,
}

impl OtaTerminators {
  fn for_kind(&self, kind: OtaKind) -> &TerminatorFn {
    match kind {
      OtaKind::Image => &self.reboot,
      OtaKind::Daemon => &self.restart_self,
    }
  }
}

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
  pub fn spawn(
    transfers: ChunkedTransfer,
    events_tx: OtaEventTx,
    terminators: OtaTerminators,
    range_proxy: RangeProxy,
  ) -> (Self, JoinHandle<()>) {
    let (cmd_tx, cmd_rx) = mpsc::channel(64);
    let actor = OtaActor {
      transfers,
      events_tx,
      terminators,
      range_proxy,
      self_tx: cmd_tx.clone(),
      cmd_rx,
      state: OtaState::Idle,
    };
    let handle = tokio::spawn(actor.run());
    (Self { cmd_tx }, handle)
  }

  pub async fn begin(&self, req: OtaBegin, peer: Option<Address>) -> Result<OtaBeginAck, OtaBeginRejected> {
    let (ack, rx) = oneshot::channel();
    if self.cmd_tx.send(Command::Begin { req, peer, ack }).await.is_err() {
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

  pub async fn asset_range_chunk(&self, chunk: OtaAssetRangeChunk) {
    if let Err(err) = self.cmd_tx.send(Command::AssetRangeChunk(chunk)).await {
      tracing::error!(?err, "ota orchestrator mailbox closed; dropping OtaAssetRangeChunk");
    }
  }
}

enum OtaState {
  Idle,
  Streaming {
    kind: OtaKind,
    update_id: String,
    expected_size: u64,
    peer: Option<Address>,
  },
  Writing {
    kind: OtaKind,
    update_id: String,
    cancel_tx: watch::Sender<bool>,
    peer: Option<Address>,
  },
}

impl OtaState {
  fn pinned_peer(&self) -> Option<Option<Address>> {
    match self {
      OtaState::Idle => None,
      OtaState::Streaming { peer, .. } | OtaState::Writing { peer, .. } => Some(*peer),
    }
  }
}

struct OtaActor {
  transfers: ChunkedTransfer,
  events_tx: OtaEventTx,
  terminators: OtaTerminators,
  range_proxy: RangeProxy,
  self_tx: mpsc::Sender<Command>,
  cmd_rx: mpsc::Receiver<Command>,
  state: OtaState,
}

impl OtaActor {
  async fn run(mut self) {
    tracing::info!("ota orchestrator started");
    while let Some(cmd) = self.cmd_rx.recv().await {
      match cmd {
        Command::Begin { req, peer, ack } => self.handle_begin(req, peer, ack).await,
        Command::Chunk(chunk) => self.handle_chunk(chunk).await,
        Command::AssetRangeChunk(chunk) => {
          self.range_proxy.route_chunk(chunk).await;
        }
        Command::Abandon { update_id } => self.handle_abandon(update_id).await,
        Command::Cancel => self.handle_cancel().await,
        Command::WriteFinished => {
          self.state = OtaState::Idle;
          self.range_proxy.deactivate().await;
        }
      }
    }
    tracing::info!("ota orchestrator exiting");
  }

  async fn handle_begin(
    &mut self,
    req: OtaBegin,
    peer: Option<Address>,
    ack: oneshot::Sender<Result<OtaBeginAck, OtaBeginRejected>>,
  ) {
    if let OtaState::Writing { update_id, .. } = &self.state {
      let _ = ack.send(Err(OtaBeginRejected {
        reason: format!("ota write of {update_id} in progress; cancel or wait first"),
      }));
      return;
    }

    if let Some(pinned) = self.state.pinned_peer()
      && pinned != peer
    {
      let _ = ack.send(Err(OtaBeginRejected {
        reason: "ota in progress, pinned to a different companion".into(),
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
      self.range_proxy.deactivate().await;
    }

    let kind = req.kind;
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
        let update_id = req.update_id.clone();
        if matches!(kind, OtaKind::Image) {
          self.range_proxy.activate(update_id.clone(), peer).await;
        }
        self.state = OtaState::Streaming {
          kind,
          update_id,
          expected_size: req.expected_size as u64,
          peer,
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
    let (kind, current_id, expected_size, peer) = match &self.state {
      OtaState::Streaming {
        kind,
        update_id,
        expected_size,
        peer,
      } => (*kind, update_id.clone(), *expected_size, *peer),
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
        self.spawn_write(kind, current_id, peer, path).await;
      }
      Err(err) => {
        let code = transfer_error_code(&err);
        emit_error(&self.events_tx, code, format!("ota chunk: {err}")).await;
        let _ = self.transfers.abandon(current_id).await;
        self.state = OtaState::Idle;
        self.range_proxy.deactivate().await;
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
      self.range_proxy.deactivate().await;
    }
  }

  async fn handle_cancel(&mut self) {
    match &self.state {
      OtaState::Idle => tracing::debug!("cancel requested with no run in flight; ignoring"),
      OtaState::Streaming { update_id, .. } => {
        tracing::info!(%update_id, "cancel during streaming; partial retained for resume");
        self.state = OtaState::Idle;
        self.range_proxy.deactivate().await;
      }
      OtaState::Writing { kind, cancel_tx, .. } => match kind {
        OtaKind::Image => {
          tracing::info!("cancel during image write; signalling libswupdate");
          let _ = cancel_tx.send(true);
        }
        OtaKind::Daemon => {
          tracing::info!("cancel during daemon swap; rename is atomic, ignoring");
        }
      },
    }
  }

  async fn spawn_write(&mut self, kind: OtaKind, update_id: String, peer: Option<Address>, payload: PathBuf) {
    let (cancel_tx, cancel_rx) = watch::channel(false);
    self.state = OtaState::Writing {
      kind,
      update_id: update_id.clone(),
      cancel_tx,
      peer,
    };

    let events_tx = self.events_tx.clone();
    let terminator = self.terminators.for_kind(kind).clone();
    let self_tx = self.self_tx.clone();

    tokio::spawn(async move {
      let outcome = match kind {
        OtaKind::Image => run_image_write(&events_tx, &payload, cancel_rx).await,
        OtaKind::Daemon => run_daemon_swap(&events_tx, &payload, cancel_rx).await,
      };
      let _ = tokio::fs::remove_file(&payload).await;
      match outcome {
        Ok(()) => {
          tracing::info!(%update_id, ?kind, "ota write complete; firing terminator");
          (terminator)();
        }
        Err(err) => {
          tracing::warn!(?err, ?kind, "ota write terminated with error");
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
pub(crate) struct WriteError {
  pub(crate) code: OtaErrorCode,
  pub(crate) msg: String,
}

async fn run_image_write(
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

async fn run_daemon_swap(
  events_tx: &OtaEventTx,
  binary_path: &std::path::Path,
  mut cancel_rx: watch::Receiver<bool>,
) -> Result<(), WriteError> {
  if check_cancel(&mut cancel_rx) {
    return Err(WriteError {
      code: OtaErrorCode::Cancelled,
      msg: "cancelled before swap".into(),
    });
  }

  emit_progress(events_tx, OtaPhase::Writing, 0, None).await;
  daemon_swap::swap(binary_path).await.map_err(|err| WriteError {
    code: OtaErrorCode::WriteFailed,
    msg: format!("daemon swap failed: {err}"),
  })?;
  emit_progress(events_tx, OtaPhase::Writing, 100, None).await;

  emit_progress(events_tx, OtaPhase::Reboot, 0, None).await;
  Ok(())
}

pub(crate) fn check_cancel(rx: &mut watch::Receiver<bool>) -> bool {
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
    restart_self_calls: Arc<AtomicUsize>,
    _root: PathBuf,
  }

  async fn boot() -> Harness {
    let root = temp_root();
    let pending = ChunkedTransfer::init(root.clone()).await.unwrap();
    let (transfers, _xfer_handle) = pending.spawn();
    let (events_tx, events) = mpsc::channel(64);
    let reboot_calls = Arc::new(AtomicUsize::new(0));
    let restart_self_calls = Arc::new(AtomicUsize::new(0));
    let reboot_counter = reboot_calls.clone();
    let restart_counter = restart_self_calls.clone();
    let terminators = OtaTerminators {
      reboot: Arc::new(move || {
        reboot_counter.fetch_add(1, Ordering::SeqCst);
      }),
      restart_self: Arc::new(move || {
        restart_counter.fetch_add(1, Ordering::SeqCst);
      }),
    };
    let (ota, _ota_handle) = OtaOrchestrator::spawn(transfers, events_tx, terminators, range_proxy::noop_proxy());
    Harness {
      ota,
      events,
      reboot_calls,
      restart_self_calls,
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
      .begin(
        OtaBegin {
          kind: OtaKind::Image,
          update_id: sha.clone(),
          update_url_base: None,
          expected_sha256: sha.clone(),
          expected_size: size,
        },
        None,
      )
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
      .begin(
        OtaBegin {
          kind: OtaKind::Image,
          update_id: sha.clone(),
          update_url_base: None,
          expected_sha256: sha.clone(),
          expected_size: size + 1,
        },
        None,
      )
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
      .begin(
        OtaBegin {
          kind: OtaKind::Image,
          update_id: bogus_sha.clone(),
          update_url_base: None,
          expected_sha256: bogus_sha.clone(),
          expected_size: size,
        },
        None,
      )
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
      .begin(
        OtaBegin {
          kind: OtaKind::Image,
          update_id: sha.clone(),
          update_url_base: None,
          expected_sha256: sha.clone(),
          expected_size: size,
        },
        None,
      )
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
      .begin(
        OtaBegin {
          kind: OtaKind::Image,
          update_id: sha.clone(),
          update_url_base: None,
          expected_sha256: sha.clone(),
          expected_size: size,
        },
        None,
      )
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
      .begin(
        OtaBegin {
          kind: OtaKind::Image,
          update_id: sha.clone(),
          update_url_base: None,
          expected_sha256: sha.clone(),
          expected_size: size,
        },
        None,
      )
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
      .begin(
        OtaBegin {
          kind: OtaKind::Image,
          update_id: "deadbeef".repeat(8),
          update_url_base: None,
          expected_sha256: "deadbeef".repeat(8),
          expected_size: 32,
        },
        None,
      )
      .await
      .unwrap_err();
    assert!(err.reason.contains("in progress"), "got reason: {}", err.reason);
  }

  #[tokio::test]
  async fn abandon_clears_streaming_state() {
    let h = boot().await;
    let (bytes, sha, size) = fixture_bytes();
    h.ota
      .begin(
        OtaBegin {
          kind: OtaKind::Image,
          update_id: sha.clone(),
          update_url_base: None,
          expected_sha256: sha.clone(),
          expected_size: size,
        },
        None,
      )
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
      .begin(
        OtaBegin {
          kind: OtaKind::Image,
          update_id: sha.clone(),
          update_url_base: None,
          expected_sha256: sha,
          expected_size: size,
        },
        None,
      )
      .await
      .expect("begin after abandon");
    assert_eq!(ack.resume_from_offset, 0);
  }

  #[tokio::test]
  async fn second_begin_from_different_peer_is_rejected() {
    let h = boot().await;
    let (_bytes, sha, size) = fixture_bytes();
    let peer_a: Address = "AA:BB:CC:DD:EE:01".parse().unwrap();
    let peer_b: Address = "AA:BB:CC:DD:EE:02".parse().unwrap();

    h.ota
      .begin(
        OtaBegin {
          kind: OtaKind::Image,
          update_id: sha.clone(),
          update_url_base: None,
          expected_sha256: sha.clone(),
          expected_size: size,
        },
        Some(peer_a),
      )
      .await
      .expect("first begin ok");

    let err = h
      .ota
      .begin(
        OtaBegin {
          kind: OtaKind::Image,
          update_id: sha,
          update_url_base: None,
          expected_sha256: "deadbeef".repeat(8),
          expected_size: size,
        },
        Some(peer_b),
      )
      .await
      .unwrap_err();
    assert!(
      err.reason.contains("pinned to a different companion"),
      "got reason: {}",
      err.reason,
    );
  }

  // Daemon-kind. The on-device atomic-rename + systemctl restart path
  // is gated behind /etc/superbird in `daemon_swap::swap`, so the host
  // test rig sees the swap thunk no-op cleanly and the orchestrator
  // proceeds to fire the restart_self terminator. That's the contract
  // we test here: phase sequence is correct and the right terminator
  // fires.

  #[tokio::test]
  async fn daemon_happy_path_emits_subset_phases_and_restarts() {
    let mut h = boot().await;
    let (bytes, sha, size) = fixture_bytes();

    h.ota
      .begin(
        OtaBegin {
          kind: OtaKind::Daemon,
          update_id: sha.clone(),
          update_url_base: None,
          expected_sha256: sha.clone(),
          expected_size: size,
        },
        None,
      )
      .await
      .expect("daemon begin ok");

    h.ota
      .chunk(OtaChunk {
        update_id: sha,
        offset: 0,
        bytes,
        last: true,
      })
      .await;

    let mut saw_writing_done = false;
    let mut saw_reboot = false;
    let deadline = Duration::from_secs(5);
    timeout(deadline, async {
      while !(saw_writing_done && saw_reboot) {
        let ev = h.events.recv().await.expect("event channel closed");
        match ev {
          BridgeToGatewaySystemMsgEvent::OtaProgress(p) => match p.phase {
            OtaPhase::Confirming => panic!("daemon-kind must not emit Confirming"),
            OtaPhase::Writing if p.percent == 100 => saw_writing_done = true,
            OtaPhase::Reboot => saw_reboot = true,
            _ => {}
          },
          BridgeToGatewaySystemMsgEvent::OtaError(e) => panic!("unexpected error during happy path: {e:?}"),
        }
      }
    })
    .await
    .expect("daemon happy path timed out");

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
      h.restart_self_calls.load(Ordering::SeqCst),
      1,
      "restart_self thunk should fire once on daemon-kind success"
    );
    assert_eq!(
      h.reboot_calls.load(Ordering::SeqCst),
      0,
      "reboot thunk must not fire for daemon-kind"
    );
  }

  #[tokio::test]
  async fn daemon_size_mismatch_emits_size_mismatch() {
    let mut h = boot().await;
    let (bytes, sha, size) = fixture_bytes();
    h.ota
      .begin(
        OtaBegin {
          kind: OtaKind::Daemon,
          update_id: sha.clone(),
          update_url_base: None,
          expected_sha256: sha.clone(),
          expected_size: size + 1,
        },
        None,
      )
      .await
      .expect("daemon begin ok");

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
    assert_eq!(h.restart_self_calls.load(Ordering::SeqCst), 0);
  }

  #[tokio::test]
  async fn daemon_hash_mismatch_emits_hash_mismatch() {
    let mut h = boot().await;
    let (bytes, _sha, size) = fixture_bytes();
    let bogus_sha = "0".repeat(64);
    h.ota
      .begin(
        OtaBegin {
          kind: OtaKind::Daemon,
          update_id: bogus_sha.clone(),
          update_url_base: None,
          expected_sha256: bogus_sha.clone(),
          expected_size: size,
        },
        None,
      )
      .await
      .expect("daemon begin ok");
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
    assert_eq!(h.restart_self_calls.load(Ordering::SeqCst), 0);
  }

  #[tokio::test]
  async fn daemon_cancel_during_streaming_keeps_partial() {
    let h = boot().await;
    let (bytes, sha, size) = fixture_bytes();
    h.ota
      .begin(
        OtaBegin {
          kind: OtaKind::Daemon,
          update_id: sha.clone(),
          update_url_base: None,
          expected_sha256: sha.clone(),
          expected_size: size,
        },
        None,
      )
      .await
      .expect("daemon begin ok");
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
      .begin(
        OtaBegin {
          kind: OtaKind::Daemon,
          update_id: sha.clone(),
          update_url_base: None,
          expected_sha256: sha,
          expected_size: size,
        },
        None,
      )
      .await
      .expect("resume begin ok");
    assert_eq!(ack.resume_from_offset, 10, "partial should survive cancel");
    assert_eq!(h.restart_self_calls.load(Ordering::SeqCst), 0);
  }
}
