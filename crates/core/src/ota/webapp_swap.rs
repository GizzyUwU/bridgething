//! Builtin-webapp OTA backend. Extracts the staged zip bundle,
//! validates the manifest id is hub or stock, atomic-rotates the
//! existing bundle dir on the bandaid bind-mount path
//! (`/opt/bridgething/webapps/<name>`) and lets the orchestrator's
//! restart_self terminator pick up the new content.

use std::{
  io,
  path::{Path, PathBuf},
};

use libbridgething::WebappManifest;
use tokio::fs;
use uuid::Uuid;

use crate::{
  paths::{ON_DEVICE_SENTINEL, is_on_device},
  state::{HUB_WEBAPP_ID, STOCK_WEBAPP_ID, extract_zip},
};

const WEBAPPS_DIR: &str = "/opt/bridgething/webapps";

#[derive(Debug, thiserror::Error)]
pub enum SwapError {
  #[error("io error during {step}: {source}")]
  Io {
    step: &'static str,
    #[source]
    source: io::Error,
  },
  #[error("extract: {0}")]
  Extract(String),
  #[error("manifest: {0}")]
  Manifest(String),
  #[error("bundle id {id} is not a builtin webapp")]
  NotBuiltin { id: Uuid },
}

fn io_err(step: &'static str) -> impl Fn(io::Error) -> SwapError {
  move |source| SwapError::Io { step, source }
}

pub async fn swap(staged_bundle: &Path) -> Result<(), SwapError> {
  if !is_on_device() {
    tracing::warn!(
      "builtin-webapp swap requested but {ON_DEVICE_SENTINEL} is missing - no-op (off-device safety gate)"
    );
    return Ok(());
  }

  let webapps_root = PathBuf::from(WEBAPPS_DIR);
  fs::create_dir_all(&webapps_root)
    .await
    .map_err(io_err("mkdir webapps root"))?;

  for name in ["hub", "stock"] {
    let prev = webapps_root.join(format!("{name}.previous"));
    if let Err(err) = fs::remove_dir_all(&prev).await
      && err.kind() != io::ErrorKind::NotFound
    {
      return Err(SwapError::Io {
        step: "clear stale previous",
        source: err,
      });
    }
  }

  let staging = webapps_root.join(format!(".tmp.{}", Uuid::now_v7().simple()));
  fs::create_dir_all(&staging).await.map_err(io_err("mkdir staging"))?;

  if let Err(err) = run_extract(staged_bundle.to_path_buf(), staging.clone()).await {
    let _ = fs::remove_dir_all(&staging).await;
    return Err(err);
  }

  let manifest = match read_manifest(&staging).await {
    Ok(m) => m,
    Err(err) => {
      let _ = fs::remove_dir_all(&staging).await;
      return Err(err);
    }
  };

  let target_name = builtin_dir_name(manifest.id).ok_or_else(|| {
    let id = manifest.id;
    tokio::spawn({
      let staging = staging.clone();
      async move {
        let _ = fs::remove_dir_all(&staging).await;
      }
    });
    SwapError::NotBuiltin { id }
  })?;

  let final_path = webapps_root.join(target_name);
  let previous_path = webapps_root.join(format!("{target_name}.previous"));

  if fs::try_exists(&final_path).await.map_err(io_err("stat current"))? {
    fs::rename(&final_path, &previous_path)
      .await
      .map_err(io_err("rotate current -> previous"))?;
  }
  fs::rename(&staging, &final_path)
    .await
    .map_err(io_err("promote staging -> current"))?;

  tracing::info!(
    "builtin webapp '{target_name}' swapped: {} now serves the new bundle",
    final_path.display()
  );
  Ok(())
}

async fn run_extract(archive_path: PathBuf, dest: PathBuf) -> Result<(), SwapError> {
  tokio::task::spawn_blocking(move || extract_zip(&archive_path, &dest))
    .await
    .map_err(|e| SwapError::Extract(format!("extract task panicked: {e}")))?
    .map_err(|e| SwapError::Extract(format!("{e:?}")))
}

async fn read_manifest(staging: &Path) -> Result<WebappManifest, SwapError> {
  let bytes = fs::read(staging.join("manifest.json"))
    .await
    .map_err(|e| SwapError::Manifest(format!("read manifest.json: {e}")))?;
  serde_json::from_slice::<WebappManifest>(&bytes).map_err(|e| SwapError::Manifest(format!("parse manifest.json: {e}")))
}

fn builtin_dir_name(id: Uuid) -> Option<&'static str> {
  if id == HUB_WEBAPP_ID {
    Some("hub")
  } else if id == STOCK_WEBAPP_ID {
    Some("stock")
  } else {
    None
  }
}
