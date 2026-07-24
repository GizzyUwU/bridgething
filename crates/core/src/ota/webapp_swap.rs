use std::{
  io,
  path::{Path, PathBuf},
};

use libbridgething::{OtaKind, WebappManifest};
use tokio::fs;
use uuid::Uuid;

use super::staging::{self, StagePaths, StagedPiece};
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

pub async fn stage(staged_bundle: &Path, update_id: String) -> Result<StagedPiece, SwapError> {
  if !is_on_device() {
    tracing::warn!(
      "builtin-webapp stage requested but {ON_DEVICE_SENTINEL} is missing - no-op (off-device safety gate)"
    );
    return Ok(StagedPiece {
      kind: OtaKind::BuiltinWebapp,
      update_id,
      paths: None,
    });
  }

  let webapps_root = PathBuf::from(WEBAPPS_DIR);
  fs::create_dir_all(&webapps_root)
    .await
    .map_err(io_err("mkdir webapps root"))?;

  let tmp = webapps_root.join(format!(".tmp.{}", Uuid::now_v7().simple()));
  fs::create_dir_all(&tmp).await.map_err(io_err("mkdir tmp"))?;

  if let Err(err) = run_extract(staged_bundle.to_path_buf(), tmp.clone()).await {
    staging::remove_any(&tmp).await;
    return Err(err);
  }

  let manifest = match read_manifest(&tmp).await {
    Ok(m) => m,
    Err(err) => {
      staging::remove_any(&tmp).await;
      return Err(err);
    }
  };

  let target_name = match builtin_dir_name(manifest.id) {
    Some(name) => name,
    None => {
      staging::remove_any(&tmp).await;
      return Err(SwapError::NotBuiltin { id: manifest.id });
    }
  };

  let current = webapps_root.join(target_name);
  let previous = webapps_root.join(format!("{target_name}.previous"));
  let incoming = webapps_root.join(format!(".incoming.{target_name}"));

  staging::remove_any(&incoming).await;
  staging::remove_any(&previous).await;
  fs::rename(&tmp, &incoming)
    .await
    .map_err(io_err("rename tmp -> incoming"))?;

  tracing::info!("builtin webapp '{target_name}' staged at {}", incoming.display());
  Ok(StagedPiece {
    kind: OtaKind::BuiltinWebapp,
    update_id,
    paths: Some(StagePaths {
      incoming,
      current,
      previous,
    }),
  })
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
