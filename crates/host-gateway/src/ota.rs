//! OTA push driver. Reads a `.swu` from disk, hashes it, pushes via
//! `AssetCache::Push`, then sends `ApplyUpdate` referencing the same
//! id. Streams `OtaProgress` / `OtaError` events to stdout until the
//! daemon reboots or times out.

use std::{path::PathBuf, time::Duration};

use anyhow::{Context, Result, anyhow};
use libbridgething::{
  AssetRetention, TtlRetention,
  gateway::{
    ApplyUpdate, AssetPush, BridgeToGatewayMsgData, BridgeToGatewaySystemMsg, GatewayToBridgeAssetMsg,
    GatewayToBridgeMsg, GatewayToBridgeMsgData, GatewayToBridgeSystemMsg,
  },
  wire::MsgMeta,
};
use sha2::{Digest, Sha256};

use crate::{
  chaos::ChaosConfig,
  conn::{Connection, OutboundFrame},
};

const TTL_SECONDS: u32 = 2 * 60 * 60;

pub async fn run_push_update(
  url: &str,
  chaos: ChaosConfig,
  chunk_size: usize,
  swu: PathBuf,
  manifest_url: Option<String>,
  asset_id_override: Option<String>,
) -> Result<()> {
  let bytes = tokio::fs::read(&swu)
    .await
    .with_context(|| format!("read .swu {}", swu.display()))?;
  let size = u32::try_from(bytes.len()).map_err(|_| anyhow!(".swu larger than 4 GiB; refusing"))?;
  if bytes.len() > chunk_size {
    tracing::debug!(
      requested = chunk_size,
      actual = bytes.len(),
      "chunk-size knob is informational - this push is one frame"
    );
  }
  let sha256 = {
    let mut h = Sha256::new();
    h.update(&bytes);
    hex::encode(h.finalize())
  };
  let asset_id = asset_id_override.unwrap_or_else(|| format!("ota/swu/{sha256}"));
  tracing::info!(path = %swu.display(), size, sha256 = %sha256, asset_id = %asset_id, "loaded .swu");

  let mut conn = Connection::open(url, chaos).await?;
  conn.announce_version().await?;
  // wait for the daemon's Version event so we know the channel is up before pushing bulk
  let _ = await_version(&mut conn).await;

  tracing::info!("pushing asset (Bulk lane)");
  let push = AssetPush {
    id: asset_id.clone(),
    bytes,
    mime: Some("application/swu".into()),
    retention: AssetRetention::Ttl(TtlRetention { seconds: TTL_SECONDS }),
  };
  conn
    .outbound_tx
    .send(OutboundFrame::bulk(GatewayToBridgeMsg {
      id: uuid::Uuid::now_v7(),
      meta: MsgMeta::Event,
      data: GatewayToBridgeMsgData::Asset(GatewayToBridgeAssetMsg::Push(push)),
    }))
    .await
    .map_err(|_| anyhow!("connection writer closed before asset push"))?;

  tracing::info!("sending ApplyUpdate");
  let apply = ApplyUpdate {
    asset_id,
    manifest_url,
    expected_sha256: sha256,
    expected_size: size,
  };
  conn
    .outbound_tx
    .send(OutboundFrame::normal(GatewayToBridgeMsg {
      id: uuid::Uuid::now_v7(),
      meta: MsgMeta::Command,
      data: GatewayToBridgeMsgData::System(GatewayToBridgeSystemMsg::ApplyUpdate(apply)),
    }))
    .await
    .map_err(|_| anyhow!("connection writer closed before ApplyUpdate"))?;

  loop {
    match tokio::time::timeout(Duration::from_secs(300), conn.inbound_rx.recv()).await {
      Ok(Some(msg)) => match msg.data {
        BridgeToGatewayMsgData::System(BridgeToGatewaySystemMsg::OtaProgress(p)) => {
          tracing::info!(phase = ?p.phase, percent = p.percent, eta_ms = ?p.eta_ms, "progress");
          if matches!(p.phase, libbridgething::gateway::OtaPhase::Reboot) {
            tracing::info!("daemon entering reboot - exiting");
            return Ok(());
          }
        }
        BridgeToGatewayMsgData::System(BridgeToGatewaySystemMsg::OtaError(e)) => {
          tracing::error!(code = ?e.code, msg = %e.msg, "ota error");
          return Err(anyhow!("ota failed: {:?} {}", e.code, e.msg));
        }
        other => tracing::debug!(?other, "inbound (non-ota)"),
      },
      Ok(None) => {
        tracing::info!("connection closed - assuming success-by-reboot");
        return Ok(());
      }
      Err(_) => return Err(anyhow!("ota timeout (no progress in 5 min)")),
    }
  }
}

async fn await_version(conn: &mut Connection) -> Option<()> {
  // The daemon sends BridgeThingMeta as its first message on connect; bail
  // after a short wait if we don't see it, since the dev daemon may have
  // pre-existing state where the Version event was already consumed.
  let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
  loop {
    let now = tokio::time::Instant::now();
    if now >= deadline {
      return None;
    }
    let remaining = deadline - now;
    match tokio::time::timeout(remaining, conn.inbound_rx.recv()).await {
      Ok(Some(msg)) => {
        if matches!(msg.data, BridgeToGatewayMsgData::Version(_)) {
          tracing::debug!("got daemon Version, ready to push");
          return Some(());
        }
      }
      Ok(None) | Err(_) => return None,
    }
  }
}
