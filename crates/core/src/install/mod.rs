//! Shared chunked-install plumbing. Both gateway-side and client-side
//! WebappInstall surfaces route through these helpers, so the only
//! difference between "companion installs a webapp" and "webapp installs
//! a webapp" is which wire surface emits the begin/chunk/abandon - the
//! actual transfer + verify + extract + install + broadcast logic is
//! identical and lives here.
//!
//! Mirrors OTA's `Begin/Chunk/Abandon` shape. The chunked-transfer
//! primitive ([`crate::transfer::ChunkedTransfer`]) handles the on-disk
//! partial + sha256 + size verify; this module sequences install_from_path
//! and broadcasts the terminal events.

use std::path::PathBuf;

use libbridgething::{
  WebappError,
  client::BridgeToClientWebappMsgEvent,
  gateway::{BridgeToGatewayWebappMsgEvent, WebappInstallFailed as GatewayInstallFailed},
};
use tokio_util::bytes::Bytes;

use crate::{
  bluetooth::BluetoothMan,
  state::State,
  transfer::{ChunkOutcome, TransferError},
};

pub async fn install_begin(
  state: &State,
  install_id: String,
  expected_sha256: String,
  expected_size: u32,
) -> Result<u32, WebappError> {
  match state
    .transfers
    .begin(install_id, expected_size as u64, Some(expected_sha256))
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
    tracing::debug!(count = errs.len(), "webapp install failed client broadcast non-fatal errors");
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
