//! Backend dispatch for the OTA write phase. With the `swupdate`
//! cargo feature, [`install_swu`] hands the on-disk `.swu` to
//! libswupdate via the FFI; without it, [`install_swu`] is a stub
//! that emits scripted progress so the rest of the OTA flow can be
//! exercised in dev. Both paths take the file path of the
//! already-on-disk `.swu` (the ChunkedTransfer partial) and stream
//! it from disk - bytes never accumulate in memory.

mod stub;

#[cfg(feature = "swupdate")]
mod ffi;

use std::path::Path;

use libbridgething::OtaPhase;
use tokio::sync::watch;

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // explicitly allowed dead_code so dev builds won't warn
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
#[allow(dead_code)] // explicitly allowed dead_code so dev builds won't warn
pub struct Selector {
  pub software_set: String,
  pub running_mode: String,
}

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)] // explicitly allowed dead_code so dev builds won't warn
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
