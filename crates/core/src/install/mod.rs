//! Shared chunked-install plumbing for the gateway-side WebappInstall
//! surface: transfer + verify + extract + install + broadcast. Mirrors
//! OTA's `Begin/Chunk/Abandon` shape. The chunked-transfer primitive
//! ([`crate::transfer::ChunkedTransfer`]) handles the on-disk partial +
//! sha256 + size verify; this module sequences install_from_path and
//! broadcasts the terminal events.
//!
//! Also holds the first-boot example-webapp seeder ([`seed_examples`]),
//! which reuses `install_from_path` to install bundled samples into the
//! writable registry once.

use std::path::{Path, PathBuf};

use libbridgething::{
  WebappError,
  client::BridgeToClientWebappMsgEvent,
  gateway::{BridgeToGatewayWebappMsgEvent, WebappInstallFailed as GatewayInstallFailed},
};
use tokio_util::bytes::Bytes;

use crate::{
  bluetooth::BluetoothMan,
  state::{State, WebappRegistry},
  transfer::{ChunkOutcome, TransferError},
};

pub async fn seed_examples(webapps: &WebappRegistry, examples_dir: &Path, marker: &Path) {
  if tokio::fs::try_exists(marker).await.unwrap_or(false) {
    return;
  }

  match tokio::fs::read_dir(examples_dir).await {
    Ok(mut entries) => {
      let mut zips: Vec<PathBuf> = Vec::new();
      while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("zip") {
          zips.push(path);
        }
      }
      zips.sort();
      for zip in zips {
        match webapps.install_from_path(zip.clone()).await {
          Ok(info) => tracing::info!("seeded example webapp '{}' ({})", info.name, info.id),
          Err(err) => tracing::warn!("failed to seed example {}: {err:?}", zip.display()),
        }
      }
    }
    Err(err) => tracing::debug!("no example seed dir at {}: {err}", examples_dir.display()),
  }

  write_seed_marker(marker).await;
}

async fn write_seed_marker(marker: &Path) {
  if let Some(parent) = marker.parent() {
    let _ = tokio::fs::create_dir_all(parent).await;
  }
  if let Err(err) = tokio::fs::write(marker, b"1").await {
    tracing::warn!("failed to write example seed marker {}: {err:?}", marker.display());
  }
}

pub async fn install_begin(
  state: &State,
  install_id: String,
  expected_sha256: String,
  expected_size: u32,
) -> Result<u32, WebappError> {
  match state
    .transfers
    .begin(install_id, expected_size as u64, Some(expected_sha256), None)
    .await
  {
    Ok(resume_from_offset) => Ok(resume_from_offset.min(u32::MAX as u64) as u32),
    Err(err) => Err(transfer_err_to_webapp_err(err)),
  }
}

pub async fn accept_install_chunk(
  state: &State,
  bluetooth: &BluetoothMan,
  install_id: String,
  offset: u32,
  bytes: Vec<u8>,
  last: bool,
) {
  let chunk = Bytes::from(bytes);
  match state
    .transfers
    .accept_chunk(install_id.clone(), offset as u64, chunk, last)
    .await
  {
    Ok(ChunkOutcome::Continue { .. }) => {}
    Ok(ChunkOutcome::Completed { path, sha256: _ }) => {
      let state = state.clone();
      let bluetooth = bluetooth.clone();
      tokio::spawn(async move {
        complete_install(state, bluetooth, install_id, path).await;
      });
    }
    Err(err) => {
      let webapp_err = transfer_err_to_webapp_err(err);
      broadcast_install_failed(state, bluetooth, install_id.clone(), webapp_err).await;
      let _ = state.transfers.abandon(install_id).await;
    }
  }
}

pub async fn install_abandon(state: &State, install_id: String) {
  if let Err(err) = state.transfers.abandon(install_id.clone()).await {
    tracing::warn!(?err, install_id, "install abandon failed");
  }
}

async fn complete_install(state: State, bluetooth: BluetoothMan, install_id: String, archive_path: PathBuf) {
  let install_result = state.webapps.install_from_path(archive_path.clone()).await;

  let _ = state.transfers.abandon(install_id.clone()).await;
  let _ = tokio::fs::remove_file(&archive_path).await;

  match install_result {
    Ok(info) => {
      if let Some(manifest) = state.webapps.manifest(info.id).await
        && let Err(err) = state.kv.seed_config_defaults(&manifest).await
      {
        tracing::warn!(?err, id = %info.id, "config-default seed failed after install");
      }
      tracing::info!(install_id, id = %info.id, name = %info.name, "webapp install completed");
      broadcast_installed(&state, &bluetooth, info).await;
    }
    Err(err) => {
      tracing::warn!(install_id, ?err, "webapp install failed");
      broadcast_install_failed(&state, &bluetooth, install_id, err).await;
    }
  }
}

async fn broadcast_installed(state: &State, bluetooth: &BluetoothMan, info: libbridgething::WebappInfo) {
  let gateway_event = BridgeToGatewayWebappMsgEvent::WebappInstalled(info.clone());
  bluetooth.gateway_man.broadcast(gateway_event).await;

  let client_event = BridgeToClientWebappMsgEvent::WebappInstalled(info);
  if let Err(errs) = state.bus.broadcast_event(client_event).await {
    tracing::debug!(count = errs.len(), "webapp installed client broadcast non-fatal errors");
  }
}

async fn broadcast_install_failed(state: &State, bluetooth: &BluetoothMan, install_id: String, error: WebappError) {
  let gateway_event = BridgeToGatewayWebappMsgEvent::WebappInstallFailed(GatewayInstallFailed {
    install_id: install_id.clone(),
    error: error.clone(),
  });
  bluetooth.gateway_man.broadcast(gateway_event).await;

  let client_event = BridgeToClientWebappMsgEvent::WebappInstallFailed(libbridgething::client::WebappInstallFailed {
    install_id,
    error,
  });
  if let Err(errs) = state.bus.broadcast_event(client_event).await {
    tracing::debug!(
      count = errs.len(),
      "webapp install failed client broadcast non-fatal errors"
    );
  }
}

fn transfer_err_to_webapp_err(err: TransferError) -> WebappError {
  match err {
    TransferError::HashMismatch { .. } => WebappError::ArchiveSha256Mismatch,
    TransferError::SizeMismatch { .. } | TransferError::SizeOverflow { .. } => WebappError::ArchiveSizeMismatch,
    TransferError::UnknownTransfer { id } => WebappError::ArchiveTransferNotFound { install_id: id },
    TransferError::ConflictingBegin { id } => WebappError::ArchiveTransferNotFound { install_id: id },
    other => WebappError::Internal {
      reason: other.to_string(),
    },
  }
}

#[cfg(test)]
mod tests {
  use std::io::Write;

  use uuid::Uuid;

  use super::*;

  fn write_bundle_zip(dir: &Path, name: &str, id: &Uuid) -> PathBuf {
    let zip_path = dir.join(format!("{name}.zip"));
    let file = std::fs::File::create(&zip_path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default();
    zip.start_file("index.html", opts).unwrap();
    zip.write_all(b"<!doctype html><title>seed</title>").unwrap();
    let manifest = format!(r#"{{"id":"{id}","name":"{name}","version":"0.1.0","config":[],"permissions":[]}}"#);
    zip.start_file("manifest.json", opts).unwrap();
    zip.write_all(manifest.as_bytes()).unwrap();
    zip.finish().unwrap();
    zip_path
  }

  async fn registry(installed: &Path) -> WebappRegistry {
    let builtin = installed.with_file_name("builtin");
    std::fs::create_dir_all(&builtin).unwrap();
    std::fs::create_dir_all(installed).unwrap();
    WebappRegistry::init(installed.to_path_buf(), builtin).await.unwrap()
  }

  #[tokio::test]
  async fn seeds_once_then_gated_by_marker() {
    let root = std::env::temp_dir().join(format!("bridgething-seed-test-{}", Uuid::now_v7()));
    let examples = root.join("examples");
    let installed = root.join("webapps");
    let marker = root.join(".seeded");
    std::fs::create_dir_all(&examples).unwrap();
    let id_a = Uuid::now_v7();
    let id_b = Uuid::now_v7();
    write_bundle_zip(&examples, "alpha", &id_a);
    write_bundle_zip(&examples, "beta", &id_b);

    let reg = registry(&installed).await;
    seed_examples(&reg, &examples, &marker).await;

    assert!(reg.resolve(id_a).await.is_some(), "alpha seeded");
    assert!(reg.resolve(id_b).await.is_some(), "beta seeded");
    assert!(tokio::fs::try_exists(&marker).await.unwrap(), "marker written");

    // delete one and re-seed: the marker gates, so it must not reappear.
    let dir_a = installed.join(id_a.simple().to_string());
    tokio::fs::remove_dir_all(&dir_a).await.unwrap();
    reg.rescan().await;
    assert!(reg.resolve(id_a).await.is_none(), "alpha removed");

    seed_examples(&reg, &examples, &marker).await;
    assert!(
      reg.resolve(id_a).await.is_none(),
      "deleted example does not reappear after re-seed"
    );

    let _ = std::fs::remove_dir_all(&root);
  }

  #[tokio::test]
  async fn missing_dir_still_marks_first_boot() {
    let root = std::env::temp_dir().join(format!("bridgething-seed-empty-{}", Uuid::now_v7()));
    let installed = root.join("webapps");
    let marker = root.join(".seeded");
    let reg = registry(&installed).await;

    seed_examples(&reg, &root.join("does-not-exist"), &marker).await;
    assert!(
      tokio::fs::try_exists(&marker).await.unwrap(),
      "marker written even with no seed dir"
    );

    let _ = std::fs::remove_dir_all(&root);
  }
}
