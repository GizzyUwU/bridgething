//! Webapp-install driver over the unified OTA surface. Reads a `.zip`
//! from disk, hashes it, opens an `OtaBegin { kind: InstalledWebapp }`
//! request, then streams the bytes as `TransferFragment` events on the
//! Background lane until the daemon emits a `WebappInstalled` event
//! (success) or an `OtaError` (failure). Third-party installs share the
//! OTA orchestrator with image / daemon / builtin-webapp updates.

use std::{
  path::{Path, PathBuf},
  time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow};
use libbridgething::{
  OtaKind, Priority, WebappInfo,
  gateway::{
    BridgeToGatewayMsgData, BridgeToGatewaySystemMsg, BridgeToGatewayTransferMsg, BridgeToGatewayWebappMsg,
    GatewayToBridgeMsg, OtaBegin, OtaBeginAck, OtaBeginRejected, TransferRef,
  },
  wire::{MsgMeta, RequestError, ResponseMeta, WireRequest},
};
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;

use crate::{
  chaos::ChaosConfig,
  conn::{Connection, OutboundFrame},
  transfer::{AckRegistry, stream_file_fragments},
};

pub async fn run_install(
  url: &str,
  chaos: ChaosConfig,
  chunk_size: usize,
  bundle: PathBuf,
  provenance: Option<String>,
) -> Result<()> {
  let metadata = tokio::fs::metadata(&bundle)
    .await
    .with_context(|| format!("stat bundle {}", bundle.display()))?;
  let total_len = metadata.len();
  let size = u32::try_from(total_len).map_err(|_| anyhow!("bundle larger than 4 GiB; refusing"))?;
  let sha256 = hash_file(&bundle).await?;
  tracing::info!(path = %bundle.display(), size, sha256 = %sha256, "loaded bundle");

  let mut conn = Connection::open(url, chaos).await?;
  conn.announce_version().await?;
  let _ = await_version(&mut conn).await;

  tracing::info!("opening OtaBegin (InstalledWebapp)");
  let transfer_id = uuid::Uuid::now_v7();
  let begin = OtaBegin {
    kind: OtaKind::InstalledWebapp,
    update_id: sha256.clone(),
    update_url_base: None,
    transfer: TransferRef {
      id: transfer_id,
      total_size: size,
      sha256: Some(sha256.clone()),
    },
    patch: None,
    provenance,
  };
  let resume_from_offset = match send_begin(&mut conn, begin).await? {
    Ok(ack) => ack.resume_from_offset,
    Err(RequestError::Domain(OtaBeginRejected { reason })) => return Err(anyhow!("install rejected: {reason}")),
    Err(RequestError::Protocol(err)) => return Err(anyhow!("install begin protocol error: {err:?}")),
    Err(RequestError::ResponseMismatch) => return Err(anyhow!("install begin response shape mismatch")),
  };
  if resume_from_offset > 0 {
    tracing::info!(
      offset = resume_from_offset,
      "daemon reports partial; resuming from byte offset"
    );
  }

  let registry = AckRegistry::default();
  let window = registry.register(transfer_id, resume_from_offset as u64);
  let out = conn.outbound_tx.clone();
  let bundle_path = bundle.clone();
  let stream = tokio::spawn(async move {
    if let Err(err) = stream_file_fragments(
      &out,
      transfer_id,
      &bundle_path,
      resume_from_offset as u64,
      total_len,
      chunk_size,
      Priority::Background,
      &window,
    )
    .await
    {
      tracing::error!(?err, "install stream failed");
    }
  });
  let result = watch_for_outcome(&mut conn, &registry).await;
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

async fn await_version(conn: &mut Connection) -> Result<()> {
  let deadline = Instant::now() + Duration::from_secs(5);
  loop {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
      return Err(anyhow!("timed out waiting for daemon Version"));
    }
    let recv = tokio::time::timeout(remaining, conn.inbound_rx.recv()).await;
    let msg = match recv {
      Ok(Some(msg)) => msg,
      Ok(None) => return Err(anyhow!("connection closed before Version")),
      Err(_) => return Err(anyhow!("timed out waiting for daemon Version")),
    };
    if matches!(msg.data, BridgeToGatewayMsgData::Version(_)) {
      return Ok(());
    }
  }
}

async fn send_begin(
  conn: &mut Connection,
  begin: OtaBegin,
) -> Result<Result<OtaBeginAck, RequestError<OtaBeginRejected>>> {
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

async fn watch_for_outcome(conn: &mut Connection, registry: &AckRegistry) -> Result<()> {
  let deadline = Instant::now() + Duration::from_secs(60);
  loop {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
      return Err(anyhow!("install completion timed out after upload"));
    }
    let recv = tokio::time::timeout(remaining, conn.inbound_rx.recv()).await;
    let msg = match recv {
      Ok(Some(msg)) => msg,
      Ok(None) => return Err(anyhow!("connection closed before install outcome")),
      Err(_) => return Err(anyhow!("install completion timed out after upload")),
    };
    match msg.data {
      BridgeToGatewayMsgData::Transfer(BridgeToGatewayTransferMsg::Ack(ack)) => {
        registry.note(ack.transfer_id, ack.received as u64);
      }
      BridgeToGatewayMsgData::Webapp(BridgeToGatewayWebappMsg::WebappInstalled(info)) => {
        log_installed(&info);
        return Ok(());
      }
      BridgeToGatewayMsgData::System(BridgeToGatewaySystemMsg::OtaError(err)) => {
        return Err(anyhow!("install failed: {} ({:?})", err.msg, err.code));
      }
      other => {
        tracing::trace!(?other, "non-terminal event while awaiting install outcome");
      }
    }
  }
}

fn log_installed(info: &WebappInfo) {
  tracing::info!(
    id = %info.id,
    name = %info.name,
    version = %info.version,
    "webapp installed successfully"
  );
  println!("installed: id={} name={} version={}", info.id, info.name, info.version);
}
