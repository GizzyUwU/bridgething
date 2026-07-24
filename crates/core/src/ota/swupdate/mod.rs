mod stub;

#[cfg(feature = "swupdate")]
mod ffi;

use std::path::Path;

use libbridgething::OtaPhase;
use tokio::sync::watch;

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct ProgressTick {
  pub phase: OtaPhase,
  pub percent: u8,
  pub step: u8,
  pub nsteps: u8,
  pub dwl_percent: u8,
  pub dwl_bytes: u32,
  pub eta_ms: Option<u32>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Selector {
  pub software_set: String,
  pub running_mode: String,
}

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum Error {
  #[error("operation cancelled")]
  Cancelled,
  #[error("io error: {0}")]
  Io(#[from] std::io::Error),
  #[error("swupdate ipc error: {0}")]
  Ipc(String),
  #[error("swupdate reported failure: {0}")]
  InstallFailed(String),
}

pub async fn install_swu<F>(
  swu_path: &Path,
  selector: &Selector,
  progress: &F,
  cancel_rx: &mut watch::Receiver<bool>,
) -> Result<(), Error>
where
  F: Fn(ProgressTick) + Send + Sync,
{
  #[cfg(feature = "swupdate")]
  {
    return ffi::install_swu(swu_path, selector, progress, cancel_rx).await;
  }
  #[cfg(not(feature = "swupdate"))]
  {
    let _ = selector;
    return stub::install_swu(swu_path, progress, cancel_rx).await;
  }
}
