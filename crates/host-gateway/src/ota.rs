//! OTA push driver. Reads a `.swu` from disk, hashes it, opens an
//! `OtaBegin` request to the daemon, then streams the bytes via
//! `OtaChunk` events on the Bulk lane until the daemon emits a
//! `Reboot` phase progress event (or fails). Bytes never accumulate
//! in memory companion-side either: the file is read in `chunk_size`
//! buffers and each is shipped immediately.

use std::{
  path::PathBuf,
  time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow};
use libbridgething::{
  gateway::{
    BridgeToGatewayMsgData, BridgeToGatewaySystemMsg, GatewayToBridgeMsgData, GatewayToBridgeSystemMsg, OtaBegin,
    OtaChunk, OtaPhase,
  },
  wire::{MsgMeta, RequestError, ResponseMeta, WireRequest},
};
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;

use crate::{
  chaos::ChaosConfig,
  conn::{Connection, OutboundFrame},
};

pub async fn run_push_update(
  url: &str,
  chaos: ChaosConfig,
  chunk_size: usize,
  swu: PathBuf,
  manifest_url: Option<String>,
  _asset_id_override: Option<String>,
) -> Result<()> {
  let metadata = tokio::fs::metadata(&swu)
    .await
    .with_context(|| format!("stat .swu {}", swu.display()))?;
  let total_len = metadata.len();
  let size = u32::try_from(total_len).map_err(|_| anyhow!(".swu larger than 4 GiB; refusing"))?;
  let sha256 = hash_file(&swu).await?;
  tracing::info!(path = %swu.display(), size, sha256 = %sha256, "loaded .swu");

  let mut conn = Connection::open(url, chaos).await?;
  conn.announce_version().await?;
  let _ = await_version(&mut conn).await;

  tracing::info!("opening OtaBegin");
  let begin = OtaBegin {
    update_id: sha256.clone(),
    manifest_url,
    expected_sha256: sha256.clone(),
    expected_size: size,
  };
  let resume_from_offset = match send_begin(&mut conn, begin).await? {
    Ok(ack) => ack.resume_from_offset,
    Err(RequestError::Domain(rej)) => return Err(anyhow!("ota rejected: {}", rej.reason)),
    Err(RequestError::Protocol(err)) => return Err(anyhow!("ota begin protocol error: {err:?}")),
    Err(RequestError::ResponseMismatch) => return Err(anyhow!("ota begin response shape mismatch")),
  };
  if resume_from_offset > 0 {
    tracing::info!(
      offset = resume_from_offset,
      "daemon reports partial; resuming from byte offset"
    );
  }

  stream_chunks(&mut conn, &sha256, &swu, resume_from_offset, total_len, chunk_size).await?;
  watch_progress(&mut conn).await
}

async fn hash_file(path: &std::path::Path) -> Result<String> {
  let mut f = tokio::fs::File::open(path).await?;
  let mut h = Sha256::new();
  let mut buf = vec![0u8; 64 * 1024];
  loop {
    let n = f.read(&mut buf).await?;
    if n == 0 {
      break;
    }
    h.update(&buf[..n]);
  }
  Ok(hex::encode(h.finalize()))
}

async fn send_begin(
  conn: &mut Connection,
  begin: OtaBegin,
) -> Result<Result<libbridgething::gateway::OtaBeginAck, RequestError<libbridgething::gateway::OtaBeginRejected>>> {
  let request_id = uuid::Uuid::now_v7();
  conn
    .outbound_tx
    .send(OutboundFrame::normal(libbridgething::gateway::GatewayToBridgeMsg {
      id: request_id,
      meta: MsgMeta::Request,
      data: begin.into(),
    }))
    .await
    .map_err(|_| anyhow!("connection writer closed before OtaBegin"))?;

  let deadline = Instant::now() + Duration::from_secs(15);
  loop {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
      return Err(anyhow!("OtaBegin response timed out"));
    }
    let recv = tokio::time::timeout(remaining, conn.inbound_rx.recv()).await;
    let msg = match recv {
      Ok(Some(msg)) => msg,
      Ok(None) => return Err(anyhow!("connection closed before OtaBegin response")),
      Err(_) => return Err(anyhow!("OtaBegin response timed out")),
    };
    if let MsgMeta::Response(ResponseMeta { request_id: rid }) = &msg.meta
      && rid == &request_id
    {
      return Ok(OtaBegin::extract(msg.data));
    }
    tracing::trace!(?msg, "non-response inbound during OtaBegin wait");
  }
}

async fn stream_chunks(
  conn: &mut Connection,
  update_id: &str,
  swu: &std::path::Path,
  start_offset: u32,
  total_size: u64,
  chunk_size: usize,
) -> Result<()> {
  let mut file = tokio::fs::File::open(swu).await?;
  use tokio::io::AsyncSeekExt;
  if start_offset > 0 {
    file.seek(std::io::SeekFrom::Start(start_offset as u64)).await?;
  }

  let mut buf = vec![0u8; chunk_size];
  let mut offset: u64 = start_offset as u64;
  loop {
    let n = file.read(&mut buf).await?;
    if n == 0 {
      return Err(anyhow!(
        "unexpected EOF at offset {offset}/{total_size} before last:true",
      ));
    }
    let last = offset + n as u64 == total_size;
    let chunk = OtaChunk {
      update_id: update_id.to_string(),
      offset: u32::try_from(offset).map_err(|_| anyhow!("chunk offset overflow"))?,
      bytes: buf[..n].to_vec(),
      last,
    };
    let msg = libbridgething::gateway::GatewayToBridgeMsg {
      id: uuid::Uuid::now_v7(),
      meta: MsgMeta::Event,
      data: GatewayToBridgeMsgData::System(GatewayToBridgeSystemMsg::OtaChunk(chunk)),
    };
    conn
      .outbound_tx
      .send(OutboundFrame::bulk(msg))
      .await
      .map_err(|_| anyhow!("connection writer closed mid-stream at offset {offset}"))?;
    offset += n as u64;
    if last {
      tracing::info!(offset, "sent last chunk; awaiting completion");
      return Ok(());
    }
  }
}

async fn watch_progress(conn: &mut Connection) -> Result<()> {
  loop {
    match tokio::time::timeout(Duration::from_secs(300), conn.inbound_rx.recv()).await {
      Ok(Some(msg)) => match msg.data {
        BridgeToGatewayMsgData::System(BridgeToGatewaySystemMsg::OtaProgress(p)) => {
          tracing::info!(phase = ?p.phase, percent = p.percent, eta_ms = ?p.eta_ms, "progress");
          if matches!(p.phase, OtaPhase::Reboot) {
            tracing::info!("daemon entering reboot - exiting");
            return Ok(());
          }
        }
        BridgeToGatewayMsgData::System(BridgeToGatewaySystemMsg::OtaError(e)) => {
          tracing::error!(code = ?e.code, msg = %e.msg, "ota error");
          return Err(anyhow!("ota failed: {:?} {}", e.code, e.msg));
        }
        other => tracing::trace!(?other, "inbound (non-ota)"),
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
  let deadline = Instant::now() + Duration::from_secs(5);
  loop {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
      return None;
    }
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
