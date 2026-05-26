//! Daemon-binary backend for the OTA orchestrator. The streamed binary
//! already landed on bandaid via the orchestrator's per-kind target_dir
//! routing, so swap is a same-fs rename: stage at `bridgething.incoming`,
//! fsync, rotate existing `bridgething.current` to `bridgething.previous`,
//! rename `.incoming` to `.current`. Off-device the call short-circuits
//! behind the `/etc/superbird` sentinel and just emits a tracing warning.

use std::{
  io,
  os::unix::fs::PermissionsExt,
  path::{Path, PathBuf},
};

use tokio::fs;

use crate::paths::{ON_DEVICE_SENTINEL, is_on_device};

const DAEMON_DIR: &str = "/opt/bridgething/daemon";
const CURRENT_NAME: &str = "bridgething.current";
const PREVIOUS_NAME: &str = "bridgething.previous";
const INCOMING_NAME: &str = "bridgething.incoming";

#[derive(Debug, thiserror::Error)]
pub enum SwapError {
  #[error("io error during {step}: {source}")]
  Io {
    step: &'static str,
    #[source]
    source: io::Error,
  },
}

fn io_err(step: &'static str) -> impl Fn(io::Error) -> SwapError {
  move |source| SwapError::Io { step, source }
}

pub async fn swap(staged_binary: &Path) -> Result<(), SwapError> {
  if !is_on_device() {
    tracing::warn!("daemon swap requested but {ON_DEVICE_SENTINEL} is missing - no-op (off-device safety gate)");
    return Ok(());
  }

  let daemon_dir = PathBuf::from(DAEMON_DIR);
  let current = daemon_dir.join(CURRENT_NAME);
  let previous = daemon_dir.join(PREVIOUS_NAME);
  let incoming = daemon_dir.join(INCOMING_NAME);

  fs::create_dir_all(&daemon_dir)
    .await
    .map_err(io_err("mkdir daemon dir"))?;

  if let Err(err) = fs::remove_file(&incoming).await
    && err.kind() != io::ErrorKind::NotFound
  {
    return Err(SwapError::Io {
      step: "remove stale incoming",
      source: err,
    });
  }

  if let Err(err) = fs::remove_file(&previous).await
    && err.kind() != io::ErrorKind::NotFound
  {
    return Err(SwapError::Io {
      step: "clear stale previous",
      source: err,
    });
  }

  fs::rename(staged_binary, &incoming)
    .await
    .map_err(io_err("rename staged -> incoming"))?;

  fs::set_permissions(&incoming, std::fs::Permissions::from_mode(0o755))
    .await
    .map_err(io_err("chmod incoming"))?;
  sync_file(&incoming).await?;

  if fs::try_exists(&current).await.map_err(io_err("stat current"))? {
    fs::rename(&current, &previous)
      .await
      .map_err(io_err("rotate current -> previous"))?;
  }
  fs::rename(&incoming, &current)
    .await
    .map_err(io_err("promote incoming -> current"))?;

  tracing::info!(
    "daemon binary swapped: {} now points at the new build",
    current.display()
  );
  Ok(())
}

async fn sync_file(path: &Path) -> Result<(), SwapError> {
  let f = fs::OpenOptions::new()
    .read(true)
    .open(path)
    .await
    .map_err(io_err("open incoming for fsync"))?;
  f.sync_all().await.map_err(io_err("fsync incoming"))?;
  Ok(())
}
