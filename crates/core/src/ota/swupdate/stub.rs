use std::path::Path;

use libbridgething::OtaPhase;
use tokio::{sync::watch, time::Duration};

use super::{Error, ProgressTick, Selector};

pub async fn install_swu<F>(
  swu_path: &Path,
  selector: &Selector,
  progress: &F,
  cancel_rx: &mut watch::Receiver<bool>,
) -> Result<(), Error>
where
  F: Fn(ProgressTick) + Send + Sync,
{
  let metadata = tokio::fs::metadata(swu_path).await?;
  tracing::info!(
    path = %swu_path.display(),
    bytes = metadata.len(),
    software_set = %selector.software_set,
    running_mode = %selector.running_mode,
    "swupdate stub: would install .swu"
  );

  for tick in 0..=10u8 {
    if *cancel_rx.borrow_and_update() {
      tracing::info!("swupdate stub: cancellation observed");
      return Err(Error::Cancelled);
    }
    let percent = tick * 10;
    progress(ProgressTick {
      phase: OtaPhase::Writing,
      percent,
      step: 1,
      nsteps: 1,
      dwl_percent: 0,
      dwl_bytes: 0,
      eta_ms: Some((10 - tick) as u32 * 100),
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
  }

  tracing::warn!(
    "swupdate stub completed (no real install performed); flip to the swupdate cargo feature \
     for the real FFI path before relying on this for actual OTA"
  );
  Ok(())
}
