//! Stub backend. Emits fake progress so the orchestrator + handler +
//! host gateway pieces can be exercised end-to-end without a working
//! libswupdate. The .swu is already on disk at `swu_path` (the
//! ChunkedTransfer partial); the stub doesn't touch it - just ticks
//! 0..=100 over a fixed-duration window. Cancelable.

use std::path::Path;

use libbridgething::OtaPhase;
use tokio::{sync::watch, time::Duration};

use super::Error;

pub async fn install_swu<F>(swu_path: &Path, progress: &F, cancel_rx: &mut watch::Receiver<bool>) -> Result<(), Error>
where
  F: Fn(OtaPhase, u8, Option<u32>) + Send + Sync,
{
  let metadata = tokio::fs::metadata(swu_path).await?;
  tracing::info!(path = %swu_path.display(), bytes = metadata.len(), "swupdate stub: would install .swu");

  for tick in 0..=10u8 {
    if *cancel_rx.borrow_and_update() {
      tracing::info!("swupdate stub: cancellation observed");
      return Err(Error::Cancelled);
    }
    let percent = tick * 10;
    let eta_ms = Some((10 - tick) as u32 * 100);
    progress(OtaPhase::Writing, percent, eta_ms);
    tokio::time::sleep(Duration::from_millis(100)).await;
  }

  tracing::warn!(
    "swupdate stub completed (no real install performed); flip to the swupdate cargo feature \
     for the real FFI path before relying on this for actual OTA"
  );
  Ok(())
}
