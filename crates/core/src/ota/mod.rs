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

use crate::asset::{AssetCache, AssetCacheEvent};

pub type OtaEventTx = mpsc::Sender<BridgeToGatewaySystemMsgEvent>;

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
    events_tx: OtaEventTx,
    swu_workdir: PathBuf,
    reboot: RebootFn,
  ) -> (Self, JoinHandle<()>) {
    let (cmd_tx, cmd_rx) = mpsc::channel(8);
    let actor = OtaActor {
      assets,
      events_tx,
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
  events_tx: OtaEventTx,
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
            emit_error(
              &self.events_tx,
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
    let events_tx = self.events_tx.clone();
    let swu_workdir = self.swu_workdir.clone();
    let reboot = self.reboot.clone();
    let self_tx = self.self_tx.clone();

    tokio::spawn(async move {
      let outcome = run_apply(assets, &events_tx, &swu_workdir, req, cancel_rx).await;
      match outcome {
        Ok(()) => {
          tracing::info!("ota run completed; triggering reboot");
          (reboot)();
        }
        Err(err) => {
          tracing::warn!(?err, "ota run terminated with error");
          emit_error(&events_tx, err.code, err.msg).await;
        }
      }
      let _ = self_tx.send(Command::RunFinished).await;
    });
  }
}

async fn emit_error(events_tx: &OtaEventTx, code: OtaErrorCode, msg: String) {
  let _ = events_tx
    .send(BridgeToGatewaySystemMsgEvent::OtaError(OtaError { code, msg }))
    .await;
}

#[derive(Debug)]
struct RunError {
  code: OtaErrorCode,
  msg: String,
}

async fn run_apply(
  assets: AssetCache,
  events_tx: &OtaEventTx,
  swu_workdir: &std::path::Path,
  req: ApplyUpdate,
  mut cancel_rx: watch::Receiver<bool>,
) -> Result<(), RunError> {
  emit_progress(events_tx, OtaPhase::Downloading, 0, None).await;

  let cached = await_asset(&assets, &req.asset_id, &mut cancel_rx)
    .await
    .map_err(|code| RunError {
      code,
      msg: match code {
        OtaErrorCode::Cancelled => "cancelled while downloading".into(),
        _ => format!("asset {} not available within download timeout", req.asset_id),
      },
    })?;

  emit_progress(events_tx, OtaPhase::Downloading, 100, None).await;
  emit_progress(events_tx, OtaPhase::Verifying, 0, None).await;

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

  emit_progress(events_tx, OtaPhase::Verifying, 100, None).await;

  if check_cancel(&mut cancel_rx) {
    return Err(RunError {
      code: OtaErrorCode::Cancelled,
      msg: "cancelled before writing".into(),
    });
  }

  let progress_emitter = {
    let tx = events_tx.clone();
    move |phase: OtaPhase, percent: u8, eta_ms: Option<u32>| {
      let tx = tx.clone();
      tokio::spawn(async move {
        emit_progress(&tx, phase, percent, eta_ms).await;
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

  emit_progress(events_tx, OtaPhase::Confirming, 0, None).await;
  emit_progress(events_tx, OtaPhase::Confirming, 100, None).await;

  emit_progress(events_tx, OtaPhase::Reboot, 0, None).await;

  Ok(())
}

async fn await_asset(
  assets: &AssetCache,
  id: &str,
  cancel_rx: &mut watch::Receiver<bool>,
) -> Result<crate::asset::CachedAsset, OtaErrorCode> {
  if check_cancel(cancel_rx) {
    return Err(OtaErrorCode::Cancelled);
  }
  if let Ok(Some(cached)) = assets.get(id).await {
    return Ok(cached);
  }
  let mut events = assets.subscribe();
  let deadline = tokio::time::Instant::now() + DOWNLOAD_TIMEOUT;
  loop {
    if check_cancel(cancel_rx) {
      return Err(OtaErrorCode::Cancelled);
    }
    let now = tokio::time::Instant::now();
    if now >= deadline {
      return Err(OtaErrorCode::AssetNotFound);
    }
    let remaining = deadline - now;
    tokio::select! {
      biased;
      _ = cancel_rx.changed() => continue,
      result = tokio::time::timeout(remaining, events.recv()) => match result {
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
      },
    }
  }
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

#[cfg(test)]
mod tests {
  use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
  };

  use libbridgething::{AssetRetention, TtlRetention, gateway::ApplyUpdate};
  use sha2::{Digest, Sha256};
  use tokio::time::timeout;
  use tokio_util::bytes::Bytes;

  use super::*;

  /// Fixture .swu payload + sha256/size for the happy paths.
  fn fixture_bytes() -> (Bytes, String, u32) {
    let bytes = Bytes::from_static(b"fake-swu-payload-for-orchestrator-tests");
    let sha = {
      let mut h = Sha256::new();
      h.update(&bytes);
      hex::encode(h.finalize())
    };
    let size = bytes.len() as u32;
    (bytes, sha, size)
  }

  fn temp_workdir() -> PathBuf {
    let p = std::env::temp_dir().join(format!("bridgething-ota-test-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&p).expect("create test workdir");
    p
  }

  struct Harness {
    ota: OtaOrchestrator,
    assets: AssetCache,
    events: mpsc::Receiver<BridgeToGatewaySystemMsgEvent>,
    reboot_calls: Arc<AtomicUsize>,
    _workdir: PathBuf,
  }

  async fn boot() -> Harness {
    let db = crate::db::open(None).await.expect("open in-mem db");
    let (assets, _asset_handle) = AssetCache::init(db).await.expect("init asset cache").spawn();
    let (events_tx, events) = mpsc::channel(64);
    let workdir = temp_workdir();
    let reboot_calls = Arc::new(AtomicUsize::new(0));
    let calls = reboot_calls.clone();
    let reboot: RebootFn = Arc::new(move || {
      calls.fetch_add(1, Ordering::SeqCst);
    });
    let (ota, _ota_handle) = OtaOrchestrator::spawn(assets.clone(), events_tx, workdir.clone(), reboot);
    Harness {
      ota,
      assets,
      events,
      reboot_calls,
      _workdir: workdir,
    }
  }

  /// Drain progress events until we see the predicate match or hit a timeout.
  async fn wait_for(events: &mut mpsc::Receiver<BridgeToGatewaySystemMsgEvent>,
                    deadline: Duration,
                    pred: impl Fn(&BridgeToGatewaySystemMsgEvent) -> bool) -> BridgeToGatewaySystemMsgEvent {
    timeout(deadline, async {
      loop {
        let ev = events.recv().await.expect("event channel closed");
        if pred(&ev) {
          return ev;
        }
      }
    }).await.expect("timed out waiting for matching event")
  }

  #[tokio::test]
  async fn happy_path_drives_full_phase_sequence_and_calls_reboot() {
    let mut h = boot().await;
    let (bytes, sha, size) = fixture_bytes();
    h.assets
      .insert(
        "ota/test/happy".into(),
        bytes,
        Some("application/swu".into()),
        AssetRetention::Ttl(TtlRetention { seconds: 60 }),
      )
      .await
      .expect("insert");

    h.ota
      .apply(ApplyUpdate {
        asset_id: "ota/test/happy".into(),
        manifest_url: None,
        expected_sha256: sha,
        expected_size: size,
      })
      .await;

    let reboot_event = wait_for(&mut h.events, Duration::from_secs(10), |ev| {
      matches!(
        ev,
        BridgeToGatewaySystemMsgEvent::OtaProgress(p) if matches!(p.phase, OtaPhase::Reboot)
      )
    })
    .await;
    assert!(matches!(
      reboot_event,
      BridgeToGatewaySystemMsgEvent::OtaProgress(_)
    ));
    // The reboot thunk runs after the Reboot event is sent; give the spawn a tick to land.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(h.reboot_calls.load(Ordering::SeqCst), 1);
  }

  #[tokio::test]
  async fn size_mismatch_emits_error_and_skips_reboot() {
    let mut h = boot().await;
    let (bytes, sha, size) = fixture_bytes();
    h.assets
      .insert(
        "ota/test/size".into(),
        bytes,
        None,
        AssetRetention::Ttl(TtlRetention { seconds: 60 }),
      )
      .await
      .unwrap();

    h.ota
      .apply(ApplyUpdate {
        asset_id: "ota/test/size".into(),
        manifest_url: None,
        expected_sha256: sha,
        expected_size: size + 1,
      })
      .await;

    let err = wait_for(&mut h.events, Duration::from_secs(2), |ev| {
      matches!(ev, BridgeToGatewaySystemMsgEvent::OtaError(_))
    })
    .await;
    let BridgeToGatewaySystemMsgEvent::OtaError(e) = err else { unreachable!() };
    assert_eq!(e.code, OtaErrorCode::SizeMismatch);
    assert_eq!(h.reboot_calls.load(Ordering::SeqCst), 0);
  }

  #[tokio::test]
  async fn hash_mismatch_emits_error_and_skips_reboot() {
    let mut h = boot().await;
    let (bytes, _sha, size) = fixture_bytes();
    h.assets
      .insert(
        "ota/test/hash".into(),
        bytes,
        None,
        AssetRetention::Ttl(TtlRetention { seconds: 60 }),
      )
      .await
      .unwrap();

    h.ota
      .apply(ApplyUpdate {
        asset_id: "ota/test/hash".into(),
        manifest_url: None,
        expected_sha256: "0".repeat(64),
        expected_size: size,
      })
      .await;

    let err = wait_for(&mut h.events, Duration::from_secs(2), |ev| {
      matches!(ev, BridgeToGatewaySystemMsgEvent::OtaError(_))
    })
    .await;
    let BridgeToGatewaySystemMsgEvent::OtaError(e) = err else { unreachable!() };
    assert_eq!(e.code, OtaErrorCode::HashMismatch);
    assert_eq!(h.reboot_calls.load(Ordering::SeqCst), 0);
  }

  #[tokio::test]
  async fn late_pushed_asset_is_picked_up_via_subscribe() {
    let mut h = boot().await;
    let (bytes, sha, size) = fixture_bytes();
    let assets = h.assets.clone();
    let bytes_for_push = bytes.clone();
    tokio::spawn(async move {
      tokio::time::sleep(Duration::from_millis(100)).await;
      assets
        .insert(
          "ota/test/late".into(),
          bytes_for_push,
          None,
          AssetRetention::Ttl(TtlRetention { seconds: 60 }),
        )
        .await
        .expect("late insert");
    });

    h.ota
      .apply(ApplyUpdate {
        asset_id: "ota/test/late".into(),
        manifest_url: None,
        expected_sha256: sha,
        expected_size: size,
      })
      .await;

    let reboot_event = wait_for(&mut h.events, Duration::from_secs(10), |ev| {
      matches!(
        ev,
        BridgeToGatewaySystemMsgEvent::OtaProgress(p) if matches!(p.phase, OtaPhase::Reboot)
      )
    })
    .await;
    assert!(matches!(
      reboot_event,
      BridgeToGatewaySystemMsgEvent::OtaProgress(_)
    ));
  }

  #[tokio::test]
  async fn cancel_during_download_emits_cancelled() {
    let mut h = boot().await;
    let (_bytes, sha, size) = fixture_bytes();

    h.ota
      .apply(ApplyUpdate {
        asset_id: "ota/test/never-arrives".into(),
        manifest_url: None,
        expected_sha256: sha,
        expected_size: size,
      })
      .await;
    // Drain Downloading 0 so we know the run is parked in await_asset.
    let _ = wait_for(&mut h.events, Duration::from_secs(2), |ev| {
      matches!(ev, BridgeToGatewaySystemMsgEvent::OtaProgress(p) if matches!(p.phase, OtaPhase::Downloading))
    })
    .await;

    h.ota.cancel().await;

    let err = wait_for(&mut h.events, Duration::from_secs(2), |ev| {
      matches!(ev, BridgeToGatewaySystemMsgEvent::OtaError(_))
    })
    .await;
    let BridgeToGatewaySystemMsgEvent::OtaError(e) = err else { unreachable!() };
    assert_eq!(e.code, OtaErrorCode::Cancelled);
    assert_eq!(h.reboot_calls.load(Ordering::SeqCst), 0);
  }

  #[tokio::test]
  async fn cancel_during_writing_emits_cancelled() {
    let mut h = boot().await;
    let (bytes, sha, size) = fixture_bytes();
    h.assets
      .insert(
        "ota/test/cancel-write".into(),
        bytes,
        None,
        AssetRetention::Ttl(TtlRetention { seconds: 60 }),
      )
      .await
      .unwrap();

    h.ota
      .apply(ApplyUpdate {
        asset_id: "ota/test/cancel-write".into(),
        manifest_url: None,
        expected_sha256: sha,
        expected_size: size,
      })
      .await;

    // Wait until we see the first Writing progress tick from the stub
    // backend; that proves the orchestrator is past Verifying and is
    // inside the cancelable Writing phase.
    let _ = wait_for(&mut h.events, Duration::from_secs(5), |ev| {
      matches!(ev, BridgeToGatewaySystemMsgEvent::OtaProgress(p) if matches!(p.phase, OtaPhase::Writing))
    })
    .await;

    h.ota.cancel().await;

    let err = wait_for(&mut h.events, Duration::from_secs(5), |ev| {
      matches!(ev, BridgeToGatewaySystemMsgEvent::OtaError(_))
    })
    .await;
    let BridgeToGatewaySystemMsgEvent::OtaError(e) = err else { unreachable!() };
    assert_eq!(e.code, OtaErrorCode::Cancelled);
    assert_eq!(h.reboot_calls.load(Ordering::SeqCst), 0);
  }

  #[tokio::test]
  async fn second_apply_while_running_is_rejected_internal() {
    let mut h = boot().await;
    let (_bytes, sha, size) = fixture_bytes();
    // First apply parks in await_asset since the asset never arrives.
    h.ota
      .apply(ApplyUpdate {
        asset_id: "ota/test/parked".into(),
        manifest_url: None,
        expected_sha256: sha.clone(),
        expected_size: size,
      })
      .await;
    let _ = wait_for(&mut h.events, Duration::from_secs(2), |ev| {
      matches!(ev, BridgeToGatewaySystemMsgEvent::OtaProgress(p) if matches!(p.phase, OtaPhase::Downloading))
    })
    .await;

    h.ota
      .apply(ApplyUpdate {
        asset_id: "ota/test/parked-2".into(),
        manifest_url: None,
        expected_sha256: sha,
        expected_size: size,
      })
      .await;

    let err = wait_for(&mut h.events, Duration::from_secs(2), |ev| {
      matches!(ev, BridgeToGatewaySystemMsgEvent::OtaError(_))
    })
    .await;
    let BridgeToGatewaySystemMsgEvent::OtaError(e) = err else { unreachable!() };
    assert_eq!(e.code, OtaErrorCode::Internal);
  }

  #[tokio::test]
  async fn missing_asset_times_out_to_asset_not_found() {
    // Setup before pausing the clock so sqlx pool acquire isn't fighting
    // a frozen runtime; only the orchestrator's deadline math needs the
    // controlled clock.
    let mut h = boot().await;
    let (_bytes, sha, size) = fixture_bytes();

    tokio::time::pause();

    h.ota
      .apply(ApplyUpdate {
        asset_id: "ota/test/timeout".into(),
        manifest_url: None,
        expected_sha256: sha,
        expected_size: size,
      })
      .await;

    for _ in 0..16 {
      tokio::task::yield_now().await;
    }
    tokio::time::advance(DOWNLOAD_TIMEOUT + Duration::from_secs(1)).await;
    tokio::time::resume();

    let err = wait_for(&mut h.events, Duration::from_secs(2), |ev| {
      matches!(ev, BridgeToGatewaySystemMsgEvent::OtaError(_))
    })
    .await;
    let BridgeToGatewaySystemMsgEvent::OtaError(e) = err else { unreachable!() };
    assert_eq!(e.code, OtaErrorCode::AssetNotFound);
  }
}
