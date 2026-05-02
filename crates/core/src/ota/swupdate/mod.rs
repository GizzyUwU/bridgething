//! Backend dispatch for the OTA write phase. With the `swupdate`
//! cargo feature, [`install_swu`] hands bytes to libswupdate via the
//! FFI; without it, [`install_swu`] is a stub that writes the artifact
//! to a workdir and emits scripted progress so the rest of the OTA
//! flow can be exercised in dev.

mod stub;

#[cfg(feature = "swupdate")]
mod ffi;

use std::path::Path;

use libbridgething::gateway::OtaPhase;
use tokio::sync::watch;

#[derive(Debug, thiserror::Error)]
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
  workdir: &Path,
  bytes: &[u8],
  progress: &F,
  cancel_rx: &mut watch::Receiver<bool>,
) -> Result<(), Error>
where
  F: Fn(OtaPhase, u8, Option<u32>) + Send + Sync,
{
  #[cfg(feature = "swupdate")]
  {
    let _ = workdir;
    return ffi::install_swu(bytes, progress, cancel_rx).await;
  }
  #[cfg(not(feature = "swupdate"))]
  return stub::install_swu(workdir, bytes, progress, cancel_rx).await;
}
