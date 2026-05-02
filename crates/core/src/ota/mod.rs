//! OTA orchestrator. Drives a `.swu` from "in the asset cache" to
//! "device rebooted onto the new slot," emitting `OtaProgress` events
//! at every phase transition and an `OtaError` on terminal failure.
//!
//! Phase machine:
//!     Idle --[ApplyUpdate]--> Downloading --[asset ready]--> Verifying
//!                                                             |
//!                                            [hash/size ok]   |
//!                                                             v
//!                                                         Writing --[ok]--> Confirming --> Reboot
//!
//! Cancellation is honored through the `Writing` phase; once we hit
//! `Confirming` the slot flip is committed and we don't roll back.
//!
//! Single-instance: a fresh `Apply` arriving while one is in flight
//! is rejected with `OtaErrorCode::Internal`. After a terminal Error
//! or completion the actor returns to Idle.

mod swupdate;

use std::{path::PathBuf, sync::Arc, time::Duration};

use libbridgething::gateway::{
  ApplyUpdate, BridgeToGatewaySystemMsgEvent, OtaError, OtaErrorCode, OtaPhase, OtaProgress,
};
use sha2::{Digest, Sha256};
use tokio::{
  sync::{broadcast, mpsc, watch},
  task::JoinHandle,
};

use crate::{
  asset::{AssetCache, AssetCacheEvent},
  bluetooth::BluetoothMan,
};

const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[derive(Debug)]
enum Command {
  Apply(ApplyUpdate),
  Cancel,
  RunFinished,
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
  pub fn spawn(
    assets: AssetCache,
    bluetooth: BluetoothMan,
    swu_workdir: PathBuf,
    reboot: RebootFn,
  ) -> (Self, JoinHandle<()>) {
    let (cmd_tx, cmd_rx) = mpsc::channel(8);
    let actor = OtaActor {
      assets,
      bluetooth,
      swu_workdir,
      reboot,
      self_tx: cmd_tx.clone(),
      cmd_rx,
      current: None,
    };
    let handle = tokio::spawn(actor.run());
    (Self { cmd_tx }, handle)
  }

  pub async fn apply(&self, req: ApplyUpdate) {
    if let Err(err) = self.cmd_tx.send(Command::Apply(req)).await {
      tracing::error!(?err, "ota orchestrator mailbox closed; dropping ApplyUpdate");
    }
  }

  pub async fn cancel(&self) {
    if let Err(err) = self.cmd_tx.send(Command::Cancel).await {
      tracing::error!(?err, "ota orchestrator mailbox closed; dropping CancelUpdate");
    }
  }
}

struct OtaActor {
  assets: AssetCache,
  bluetooth: BluetoothMan,
  swu_workdir: PathBuf,
  reboot: RebootFn,
  /// Self-send channel for the spawned run to post `RunFinished` so
  /// `current` is cleared back through the actor without sharing state.
  self_tx: mpsc::Sender<Command>,
  cmd_rx: mpsc::Receiver<Command>,
  /// Cancel signaller for the active run. Present iff a run is in flight.
  current: Option<watch::Sender<bool>>,
}

impl OtaActor {
  async fn run(mut self) {
    tracing::info!("ota orchestrator started");
    while let Some(cmd) = self.cmd_rx.recv().await {
      match cmd {
        Command::Apply(req) => {
          if self.current.is_some() {
            tracing::warn!("ApplyUpdate arrived while another run is in flight; rejecting");
            broadcast_error(
              &self.bluetooth,
              OtaErrorCode::Internal,
              "ota run already in flight".into(),
            )
            .await;
            continue;
          }
          self.spawn_run(req);
        }
        Command::Cancel => {
          if let Some(tx) = self.current.as_ref() {
            tracing::info!("cancel requested; signalling in-flight run");
            let _ = tx.send(true);
          } else {
            tracing::debug!("cancel requested with no run in flight; ignoring");
          }
        }
        Command::RunFinished => {
          self.current = None;
        }
      }
    }
    tracing::info!("ota orchestrator exiting");
  }

  fn spawn_run(&mut self, req: ApplyUpdate) {
    let (cancel_tx, cancel_rx) = watch::channel(false);
    self.current = Some(cancel_tx);

    let assets = self.assets.clone();
    let bluetooth = self.bluetooth.clone();
    let swu_workdir = self.swu_workdir.clone();
    let reboot = self.reboot.clone();
    let self_tx = self.self_tx.clone();

    tokio::spawn(async move {
      let outcome = run_apply(assets, &bluetooth, &swu_workdir, req, cancel_rx).await;
      match outcome {
        Ok(()) => {
          tracing::info!("ota run completed; triggering reboot");
          (reboot)();
        }
        Err(err) => {
          tracing::warn!(?err, "ota run terminated with error");
          broadcast_error(&bluetooth, err.code, err.msg).await;
        }
      }
      let _ = self_tx.send(Command::RunFinished).await;
    });
  }
}

async fn broadcast_error(bluetooth: &BluetoothMan, code: OtaErrorCode, msg: String) {
  bluetooth
    .gateway_man
    .broadcast(BridgeToGatewaySystemMsgEvent::OtaError(OtaError { code, msg }))
    .await;
}

#[derive(Debug)]
struct RunError {
  code: OtaErrorCode,
  msg: String,
}

async fn run_apply(
  assets: AssetCache,
  bluetooth: &BluetoothMan,
  swu_workdir: &std::path::Path,
  req: ApplyUpdate,
  mut cancel_rx: watch::Receiver<bool>,
) -> Result<(), RunError> {
  emit_progress(bluetooth, OtaPhase::Downloading, 0, None).await;

  let cached = await_asset(&assets, &req.asset_id).await.map_err(|code| RunError {
    code,
    msg: format!("asset {} not available within download timeout", req.asset_id),
  })?;

  if check_cancel(&mut cancel_rx) {
    return Err(RunError {
      code: OtaErrorCode::Cancelled,
      msg: "cancelled before verifying".into(),
    });
  }

  emit_progress(bluetooth, OtaPhase::Downloading, 100, None).await;
  emit_progress(bluetooth, OtaPhase::Verifying, 0, None).await;

  if cached.bytes.len() != req.expected_size as usize {
    return Err(RunError {
      code: OtaErrorCode::SizeMismatch,
      msg: format!(
        "size mismatch: expected {}, got {}",
        req.expected_size,
        cached.bytes.len()
      ),
    });
  }

  let actual_sha = {
    let mut h = Sha256::new();
    h.update(&cached.bytes);
    hex::encode(h.finalize())
  };
  if !actual_sha.eq_ignore_ascii_case(&req.expected_sha256) {
    return Err(RunError {
      code: OtaErrorCode::HashMismatch,
      msg: format!("sha256 mismatch: expected {}, got {}", req.expected_sha256, actual_sha),
    });
  }

  emit_progress(bluetooth, OtaPhase::Verifying, 100, None).await;

  if check_cancel(&mut cancel_rx) {
    return Err(RunError {
      code: OtaErrorCode::Cancelled,
      msg: "cancelled before writing".into(),
    });
  }

  let progress_emitter = {
    let bt = bluetooth.clone();
    move |phase: OtaPhase, percent: u8, eta_ms: Option<u32>| {
      let bt = bt.clone();
      tokio::spawn(async move {
        emit_progress(&bt, phase, percent, eta_ms).await;
      });
    }
  };

  swupdate::install_swu(swu_workdir, &cached.bytes, &progress_emitter, &mut cancel_rx)
    .await
    .map_err(|err| RunError {
      code: match err {
        swupdate::Error::Cancelled => OtaErrorCode::Cancelled,
        swupdate::Error::Io(_) | swupdate::Error::Ipc(_) | swupdate::Error::InstallFailed(_) => {
          OtaErrorCode::WriteFailed
        }
      },
      msg: format!("swupdate failed: {err}"),
    })?;

  emit_progress(bluetooth, OtaPhase::Confirming, 0, None).await;
  // Slot flip + try-counter reset are handled by swupdate's bootenv block and bridgething-boot-confirm.service
  emit_progress(bluetooth, OtaPhase::Confirming, 100, None).await;

  emit_progress(bluetooth, OtaPhase::Reboot, 0, None).await;

  Ok(())
}

async fn await_asset(assets: &AssetCache, id: &str) -> Result<crate::asset::CachedAsset, OtaErrorCode> {
  if let Ok(Some(cached)) = assets.get(id).await {
    return Ok(cached);
  }
  let mut events = assets.subscribe();
  let deadline = tokio::time::Instant::now() + DOWNLOAD_TIMEOUT;
  loop {
    let now = tokio::time::Instant::now();
    if now >= deadline {
      return Err(OtaErrorCode::AssetNotFound);
    }
    let remaining = deadline - now;
    match tokio::time::timeout(remaining, events.recv()).await {
      Ok(Ok(AssetCacheEvent::Ready { id: ready_id })) if ready_id == id => {
        if let Ok(Some(cached)) = assets.get(id).await {
          return Ok(cached);
        }
      }
      Ok(Ok(_)) => continue,
      Ok(Err(broadcast::error::RecvError::Lagged(_))) => {
        if let Ok(Some(cached)) = assets.get(id).await {
          return Ok(cached);
        }
      }
      Ok(Err(broadcast::error::RecvError::Closed)) => return Err(OtaErrorCode::Internal),
      Err(_) => return Err(OtaErrorCode::AssetNotFound),
    }
  }
}

fn check_cancel(rx: &mut watch::Receiver<bool>) -> bool {
  *rx.borrow_and_update()
}

async fn emit_progress(bluetooth: &BluetoothMan, phase: OtaPhase, percent: u8, eta_ms: Option<u32>) {
  bluetooth
    .gateway_man
    .broadcast(BridgeToGatewaySystemMsgEvent::OtaProgress(OtaProgress {
      phase,
      percent,
      eta_ms,
    }))
    .await;
}
