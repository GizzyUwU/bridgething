//! Stub backend. Emits fake progress so the orchestrator + handler +
//! host gateway pieces can be exercised end-to-end without a working
//! libswupdate. Writes the bytes to a workdir file (so the staged
//! artifact is observable on disk during dev) and ticks 0..=100 over
//! a fixed-duration window. Cancelable.

use std::path::{Path, PathBuf};

use libbridgething::gateway::OtaPhase;
use tokio::{fs, io::AsyncWriteExt, sync::watch, time::Duration};

use super::Error;

// this allow is explicitly to prevent warning during prod builds
#[allow(dead_code)]
pub async fn install_swu<F>(
  workdir: &Path,
  bytes: &[u8],
  progress: &F,
  cancel_rx: &mut watch::Receiver<bool>,
) -> Result<(), Error>
where
  F: Fn(OtaPhase, u8, Option<u32>) + Send + Sync,
{
  fs::create_dir_all(workdir).await?;
  let target: PathBuf = workdir.join("pending.swu");
  let mut f = fs::File::create(&target).await?;
  f.write_all(bytes).await?;
  f.flush().await?;
  drop(f);

  tracing::info!(path = %target.display(), bytes = bytes.len(), "swupdate stub: wrote .swu to workdir");

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
