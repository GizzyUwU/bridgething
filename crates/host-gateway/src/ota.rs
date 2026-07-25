//! OTA push driver. Reads a `.swu` from disk, hashes it, opens an
//! `OtaBegin` request to the daemon, then streams the bytes as
//! `TransferFragment` events on the Background lane until the daemon
//! emits a `Reboot` phase progress event (or fails). Bytes never
//! accumulate in memory companion-side either: the file is read in
//! `chunk_size` buffers and each is shipped immediately.
//!
//! When `--zck <path>` is supplied, the same loop also services
//! inbound `OtaAssetRange` requests from the daemon's range proxy.
//! The host serves the requested byte ranges from the local .zck file
//! through the reply's `TransferBody`: small results inline, larger
//! ones as a stream-relative fragment stream on the Background lane.
//! This is the test rig for the wireless-OTA path: every wire type the
//! mobile companion will eventually serve, the host serves first.

use std::{
  collections::HashMap,
  path::{Path, PathBuf},
  sync::Arc,
  time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow};
use libbridgething::{
  OtaKind, OtaPhase, Priority,
  gateway::{
    BridgeToGatewayMsg, BridgeToGatewayMsgData, BridgeToGatewaySystemMsg, BridgeToGatewayTransferMsg,
    GatewayToBridgeMsg, GatewayToBridgeMsgData, GatewayToBridgeSystemMsg, OtaAssetRange, OtaAssetRangeRejected,
    OtaAssetRangeReply, OtaBegin, TransferBody, TransferRef,
  },
  wire::{MsgMeta, RequestError, ResponseMeta, WireRequest},
};
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;

use crate::{
  chaos::ChaosConfig,
  conn::{Connection, OutboundFrame},
  transfer::{AckRegistry, stream_file_fragments, stream_range_fragments},
};

const RANGE_CHUNK_BYTES: usize = 64 * 1024;
const RANGE_INLINE_MAX_BYTES: u32 = 16 * 1024;

pub async fn run_push_update(
  url: &str,
  chaos: ChaosConfig,
  chunk_size: usize,
  kind: OtaKind,
  artifact: PathBuf,
  update_url_base: Option<String>,
  zcks: HashMap<String, PathBuf>,
) -> Result<()> {
  let metadata = tokio::fs::metadata(&artifact)
    .await
    .with_context(|| format!("stat artifact {}", artifact.display()))?;
  let total_len = metadata.len();
  let size = u32::try_from(total_len).map_err(|_| anyhow!("artifact larger than 4 GiB; refusing"))?;
  let sha256 = hash_file(&artifact).await?;
  tracing::info!(path = %artifact.display(), ?kind, size, sha256 = %sha256, "loaded artifact");

  for (asset, path) in &zcks {
    let meta = tokio::fs::metadata(path)
      .await
      .with_context(|| format!("stat .zck {}", path.display()))?;
    tracing::info!(%asset, path = %path.display(), size = meta.len(), "loaded .zck for range serving");
  }

  let mut conn = Connection::open(url, chaos).await?;
  conn.announce_version().await?;
  let _ = await_version(&mut conn).await;

  tracing::info!(?kind, "opening OtaBegin");
  let transfer_id = uuid::Uuid::now_v7();
  let begin = OtaBegin {
    kind,
    update_id: sha256.clone(),
    update_url_base,
    transfer: TransferRef {
      id: transfer_id,
      total_size: size,
      sha256: Some(sha256.clone()),
    },
    patch: None,
    provenance: None,
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

  let registry = AckRegistry::default();
  let push_window = registry.register(transfer_id, resume_from_offset as u64);
  let out = conn.outbound_tx.clone();
  let artifact_path = artifact.clone();
  let stream = tokio::spawn(async move {
    if let Err(err) = stream_file_fragments(
      &out,
      transfer_id,
      &artifact_path,
      resume_from_offset as u64,
      total_len,
      chunk_size,
      Priority::Background,
      &push_window,
    )
    .await
    {
      tracing::error!(?err, "push stream failed");
    }
  });

  let result = run_event_loop(&mut conn, registry, Arc::new(zcks), chunk_size).await;
  stream.abort();
  result
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

async fn run_event_loop(
  conn: &mut Connection,
  registry: AckRegistry,
  zcks: Arc<HashMap<String, PathBuf>>,
  chunk_size: usize,
) -> Result<()> {
  loop {
    match tokio::time::timeout(Duration::from_secs(300), conn.inbound_rx.recv()).await {
      Ok(Some(msg)) => {
        if let Some(action) = handle_inbound(msg, conn.outbound_tx.clone(), &registry, zcks.clone(), chunk_size).await?
        {
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

async fn handle_inbound(
  msg: BridgeToGatewayMsg,
  out: tokio::sync::mpsc::Sender<OutboundFrame>,
  registry: &AckRegistry,
  zcks: Arc<HashMap<String, PathBuf>>,
  chunk_size: usize,
) -> Result<Option<Result<()>>> {
  let request_id = msg.id;
  match msg.data {
    BridgeToGatewayMsgData::Transfer(BridgeToGatewayTransferMsg::Ack(ack)) => {
      registry.note(ack.transfer_id, ack.received as u64);
    }
    BridgeToGatewayMsgData::System(BridgeToGatewaySystemMsg::OtaProgress(p)) => {
      tracing::info!(
        phase = ?p.phase,
        percent = p.percent,
        dwl_percent = p.dwl_percent,
        dwl_bytes = p.dwl_bytes,
        eta_ms = ?p.eta_ms,
        "progress"
      );
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
      let zcks = zcks.clone();
      let registry = registry.clone();
      tokio::spawn(async move {
        if let Err(err) = serve_range_request(request_id, req, out, &registry, zcks, chunk_size).await {
          tracing::warn!(?err, "OtaAssetRange handler failed");
        }
        registry.deregister(request_id);
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
  registry: &AckRegistry,
  zcks: Arc<HashMap<String, PathBuf>>,
  chunk_size: usize,
) -> Result<()> {
  let zck_path = match zcks.get(&req.asset) {
    Some(p) => p.clone(),
    None => {
      tracing::warn!(
        update_id = %req.update_id,
        asset = %req.asset,
        "OtaAssetRange for an asset with no configured .zck; rejecting",
      );
      respond_rejected(
        &out,
        request_id,
        format!("host-gateway has no .zck for asset {}", req.asset),
      )
      .await?;
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
  let stream_len: u32 = parts.iter().map(|p| p.length).sum();
  let ranges: Vec<(u32, u32)> = parts.iter().map(|p| (p.start, p.length)).collect();

  if stream_len <= RANGE_INLINE_MAX_BYTES {
    let mut body = Vec::with_capacity(stream_len as usize);
    let mut file = tokio::fs::File::open(&zck_path).await?;
    for (start, length) in &ranges {
      use tokio::io::AsyncSeekExt;
      file.seek(std::io::SeekFrom::Start(*start as u64)).await?;
      let mut piece = vec![0u8; *length as usize];
      file.read_exact(&mut piece).await?;
      body.extend_from_slice(&piece);
    }
    respond_reply(
      &out,
      request_id,
      OtaAssetRangeReply {
        total_size,
        parts,
        body: TransferBody::Inline(body),
      },
    )
    .await?;
    return Ok(());
  }

  respond_reply(
    &out,
    request_id,
    OtaAssetRangeReply {
      total_size,
      parts,
      body: TransferBody::Stream(TransferRef {
        id: request_id,
        total_size: stream_len,
        sha256: None,
      }),
    },
  )
  .await?;

  let window = registry.register(request_id, 0);
  stream_range_fragments(
    &out,
    request_id,
    &zck_path,
    &ranges,
    chunk_size.min(RANGE_CHUNK_BYTES),
    Priority::Background,
    &window,
  )
  .await?;
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
