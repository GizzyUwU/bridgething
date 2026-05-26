//! OTA push driver. Reads a `.swu` from disk, hashes it, opens an
//! `OtaBegin` request to the daemon, then streams the bytes via
//! `OtaChunk` events on the Bulk lane until the daemon emits a
//! `Reboot` phase progress event (or fails). Bytes never accumulate
//! in memory companion-side either: the file is read in `chunk_size`
//! buffers and each is shipped immediately.
//!
//! When `--zck <path>` is supplied, the same loop also services
//! inbound `OtaAssetRange` requests from the daemon's range proxy.
//! The host reads the requested byte ranges from the local .zck file
//! and streams them back as `OtaAssetRangeChunk` events on the Bulk
//! lane. This is the test rig for the wireless-OTA path: every wire
//! type the mobile companion will eventually serve, the host serves
//! first.

use std::{
  path::{Path, PathBuf},
  sync::Arc,
  time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow};
use libbridgething::{
  OtaKind, OtaPhase,
  gateway::{
    BridgeToGatewayMsg, BridgeToGatewayMsgData, BridgeToGatewaySystemMsg, GatewayToBridgeMsg, GatewayToBridgeMsgData,
    GatewayToBridgeSystemMsg, OtaAssetRange, OtaAssetRangeChunk, OtaAssetRangeRejected, OtaAssetRangeReply, OtaBegin,
    OtaChunk,
  },
  wire::{MsgMeta, RequestError, ResponseMeta, WireRequest},
};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::{
  chaos::ChaosConfig,
  conn::{Connection, OutboundFrame},
};

/// 64 KiB matches the daemon's ChunkedTransfer write granularity.
const RANGE_CHUNK_BYTES: usize = 64 * 1024;

pub async fn run_push_update(
  url: &str,
  chaos: ChaosConfig,
  chunk_size: usize,
  kind: OtaKind,
  artifact: PathBuf,
  update_url_base: Option<String>,
  zck: Option<PathBuf>,
) -> Result<()> {
  let metadata = tokio::fs::metadata(&artifact)
    .await
    .with_context(|| format!("stat artifact {}", artifact.display()))?;
  let total_len = metadata.len();
  let size = u32::try_from(total_len).map_err(|_| anyhow!("artifact larger than 4 GiB; refusing"))?;
  let sha256 = hash_file(&artifact).await?;
  tracing::info!(path = %artifact.display(), ?kind, size, sha256 = %sha256, "loaded artifact");

  if let Some(z) = zck.as_deref() {
    let meta = tokio::fs::metadata(z)
      .await
      .with_context(|| format!("stat .zck {}", z.display()))?;
    tracing::info!(path = %z.display(), size = meta.len(), "loaded .zck for range serving");
  }

  let mut conn = Connection::open(url, chaos).await?;
  conn.announce_version().await?;
  let _ = await_version(&mut conn).await;

  tracing::info!(?kind, "opening OtaBegin");
  let begin = OtaBegin {
    kind,
    update_id: sha256.clone(),
    update_url_base,
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

  stream_chunks(&mut conn, &sha256, &artifact, resume_from_offset, total_len, chunk_size).await?;
  watch_progress(&mut conn, zck.map(Arc::new)).await
}

async fn hash_file(path: &Path) -> Result<String> {
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
    .send(OutboundFrame::normal(GatewayToBridgeMsg {
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
  artifact: &Path,
  start_offset: u32,
  total_size: u64,
  chunk_size: usize,
) -> Result<()> {
  let mut file = tokio::fs::File::open(artifact).await?;
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
    let msg = GatewayToBridgeMsg {
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

async fn watch_progress(conn: &mut Connection, zck: Option<Arc<PathBuf>>) -> Result<()> {
  loop {
    match tokio::time::timeout(Duration::from_secs(300), conn.inbound_rx.recv()).await {
      Ok(Some(msg)) => {
        if let Some(action) = handle_inbound(msg, conn.outbound_tx.clone(), zck.clone()).await? {
          return action;
        }
      }
      Ok(None) => {
        tracing::info!("connection closed - assuming success-by-reboot");
        return Ok(());
      }
      Err(_) => return Err(anyhow!("ota timeout (no progress in 5 min)")),
    }
  }
}

/// Returns `Some(action)` when the loop should stop (Ok = clean exit,
/// Err = ota failure), or `None` to keep watching.
async fn handle_inbound(
  msg: BridgeToGatewayMsg,
  out: tokio::sync::mpsc::Sender<OutboundFrame>,
  zck: Option<Arc<PathBuf>>,
) -> Result<Option<Result<()>>> {
  let request_id = msg.id;
  match msg.data {
    BridgeToGatewayMsgData::System(BridgeToGatewaySystemMsg::OtaProgress(p)) => {
      tracing::info!(phase = ?p.phase, percent = p.percent, eta_ms = ?p.eta_ms, "progress");
      if matches!(p.phase, OtaPhase::Reboot) {
        tracing::info!("daemon entering reboot - exiting");
        return Ok(Some(Ok(())));
      }
    }
    BridgeToGatewayMsgData::System(BridgeToGatewaySystemMsg::OtaError(e)) => {
      tracing::error!(code = ?e.code, msg = %e.msg, "ota error");
      return Ok(Some(Err(anyhow!("ota failed: {:?} {}", e.code, e.msg))));
    }
    BridgeToGatewayMsgData::System(BridgeToGatewaySystemMsg::OtaAssetRange(req)) => {
      let zck_path = zck.clone();
      tokio::spawn(async move {
        if let Err(err) = serve_range_request(request_id, req, out, zck_path).await {
          tracing::warn!(?err, "OtaAssetRange handler failed");
        }
      });
    }
    other => tracing::trace!(?other, "inbound (non-ota)"),
  }
  Ok(None)
}

async fn serve_range_request(
  request_id: uuid::Uuid,
  req: OtaAssetRange,
  out: tokio::sync::mpsc::Sender<OutboundFrame>,
  zck: Option<Arc<PathBuf>>,
) -> Result<()> {
  let zck_path = match zck.as_deref() {
    Some(p) => p.clone(),
    None => {
      tracing::warn!(
        update_id = %req.update_id,
        "OtaAssetRange received but --zck was not supplied; rejecting",
      );
      respond_rejected(&out, request_id, "host-gateway has no .zck cached".into()).await?;
      return Ok(());
    }
  };

  let metadata = tokio::fs::metadata(&zck_path).await.with_context(|| {
    format!(
      "stat zck for OtaAssetRange (update_id={}, asset={})",
      req.update_id, req.asset,
    )
  })?;
  let total_size = u32::try_from(metadata.len()).map_err(|_| anyhow!("zck larger than 4 GiB; refusing"))?;

  for r in &req.ranges {
    let end = r.start.checked_add(r.length);
    if end.map(|e| e > total_size).unwrap_or(true) {
      respond_rejected(
        &out,
        request_id,
        format!("range {}+{} exceeds zck size {}", r.start, r.length, total_size),
      )
      .await?;
      return Ok(());
    }
  }

  let parts: Vec<libbridgething::RangePart> = req
    .ranges
    .iter()
    .map(|r| libbridgething::RangePart {
      start: r.start,
      length: r.length,
    })
    .collect();
  respond_reply(
    &out,
    request_id,
    OtaAssetRangeReply {
      total_size,
      parts: parts.clone(),
    },
  )
  .await?;

  let mut file = tokio::fs::File::open(&zck_path).await?;
  let mut buf = vec![0u8; RANGE_CHUNK_BYTES];
  for (idx, part) in parts.iter().enumerate() {
    file.seek(std::io::SeekFrom::Start(part.start as u64)).await?;
    let mut produced: u32 = 0;
    while produced < part.length {
      let want = (part.length - produced) as usize;
      let to_read = want.min(buf.len());
      let n = file.read(&mut buf[..to_read]).await?;
      if n == 0 {
        return Err(anyhow!(
          "unexpected EOF reading zck part {idx} at offset {}",
          part.start as u64 + produced as u64,
        ));
      }
      let absolute_offset = part.start + produced;
      produced += n as u32;
      let last = idx + 1 == parts.len() && produced == part.length;
      let chunk = OtaAssetRangeChunk {
        request_id,
        part_index: idx as u32,
        offset: absolute_offset,
        bytes: buf[..n].to_vec(),
        last,
      };
      let msg = GatewayToBridgeMsg {
        id: uuid::Uuid::now_v7(),
        meta: MsgMeta::Event,
        data: GatewayToBridgeMsgData::System(GatewayToBridgeSystemMsg::OtaAssetRangeChunk(chunk)),
      };
      out
        .send(OutboundFrame::bulk(msg))
        .await
        .map_err(|_| anyhow!("connection writer closed while serving range part {idx}"))?;
    }
  }
  Ok(())
}

async fn respond_reply(
  out: &tokio::sync::mpsc::Sender<OutboundFrame>,
  request_id: uuid::Uuid,
  reply: OtaAssetRangeReply,
) -> Result<()> {
  out
    .send(OutboundFrame::normal(GatewayToBridgeMsg {
      id: uuid::Uuid::now_v7(),
      meta: MsgMeta::Response(ResponseMeta { request_id }),
      data: GatewayToBridgeMsgData::System(GatewayToBridgeSystemMsg::OtaAssetRangeReply(reply)),
    }))
    .await
    .map_err(|_| anyhow!("connection writer closed before OtaAssetRangeReply send"))?;
  Ok(())
}

async fn respond_rejected(
  out: &tokio::sync::mpsc::Sender<OutboundFrame>,
  request_id: uuid::Uuid,
  reason: String,
) -> Result<()> {
  out
    .send(OutboundFrame::normal(GatewayToBridgeMsg {
      id: uuid::Uuid::now_v7(),
      meta: MsgMeta::Response(ResponseMeta { request_id }),
      data: GatewayToBridgeMsgData::System(GatewayToBridgeSystemMsg::OtaAssetRangeRejected(OtaAssetRangeRejected {
        reason,
      })),
    }))
    .await
    .map_err(|_| anyhow!("connection writer closed before OtaAssetRangeRejected send"))?;
  Ok(())
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
