use libbridgething::{
  AssetRetention,
  gateway::{
    ASSET_PUSH_SINGLE_FRAME_MAX_BYTES, AssetClear, AssetPush, AssetPushAbandon, AssetPushBegin, AssetPushBeginAck,
    AssetPushBeginRejected, AssetPushChunk, GatewayToBridgeAssetMsgCommand, GatewayToBridgeAssetMsgEvent,
    GatewayToBridgeAssetMsgRequest,
  },
};
use tokio_util::bytes::Bytes;

use super::{HandlerResult, MsgHandle};
use crate::transfer::ChunkOutcome;

#[derive(Debug)]
pub struct AssetHandler {
  handle: MsgHandle,
}

impl AssetHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle_event(self, ev: GatewayToBridgeAssetMsgEvent) -> HandlerResult {
    match ev {
      GatewayToBridgeAssetMsgEvent::Push(push) => self.handle_push(push).await,
      GatewayToBridgeAssetMsgEvent::Clear(clear) => self.handle_clear(clear).await,
      GatewayToBridgeAssetMsgEvent::PushChunk(chunk) => self.handle_push_chunk(chunk).await,
    }
  }

  pub async fn handle_command(self, cmd: GatewayToBridgeAssetMsgCommand) -> HandlerResult {
    match cmd {
      GatewayToBridgeAssetMsgCommand::PushAbandon(req) => self.handle_push_abandon(req).await,
    }
  }

  pub async fn handle_request(self, req: GatewayToBridgeAssetMsgRequest) -> HandlerResult {
    match req {
      GatewayToBridgeAssetMsgRequest::PushBegin(begin) => self.handle_push_begin(begin).await,
    }
  }

  async fn handle_push(&self, push: AssetPush) -> HandlerResult {
    let AssetPush {
      id,
      bytes,
      mime,
      retention,
    } = push;
    tracing::debug!(
      ?retention,
      mime = mime.as_deref().unwrap_or("none"),
      bytes = bytes.len(),
      id = %id,
      "({:?}) single-frame asset push from gateway",
      &self.handle.address,
    );

    if bytes.len() > ASSET_PUSH_SINGLE_FRAME_MAX_BYTES {
      tracing::warn!(
        id = %id,
        size = bytes.len(),
        cap = ASSET_PUSH_SINGLE_FRAME_MAX_BYTES,
        "rejecting single-frame Push exceeding {} byte cap; companion must use chunked PushBegin",
        ASSET_PUSH_SINGLE_FRAME_MAX_BYTES,
      );
      return Ok(());
    }
    if matches!(retention, AssetRetention::Persistent) {
      tracing::warn!(
        id = %id,
        "rejecting single-frame Push with Persistent retention; companion must use chunked PushBegin",
      );
      return Ok(());
    }

    if let Err(err) = self
      .handle
      .state
      .assets
      .insert(id, Bytes::from(bytes), mime, retention)
      .await
    {
      tracing::error!(?err, "asset cache insert failed");
    }
    Ok(())
  }

  async fn handle_clear(&self, clear: AssetClear) -> HandlerResult {
    tracing::debug!(id = %clear.id, "({:?}) asset clear from gateway", &self.handle.address);
    if let Err(err) = self.handle.state.assets.clear(&clear.id).await {
      tracing::error!(?err, "asset cache clear failed");
    }
    Ok(())
  }

  async fn handle_push_begin(self, begin: AssetPushBegin) -> HandlerResult {
    let AssetPushBegin {
      id,
      expected_size,
      expected_sha256,
      mime,
      retention,
    } = begin;
    tracing::info!(
      id = %id,
      expected_size,
      ?retention,
      "({:?}) AssetPushBegin from gateway",
      &self.handle.address,
    );

    let pending = PendingPush { mime, retention };
    PENDING_PUSHES.with_pending(id.clone(), pending);

    match self
      .handle
      .state
      .transfers
      .begin(id.clone(), expected_size as u64, expected_sha256)
      .await
    {
      Ok(resume_from_offset) => {
        self
          .handle
          .respond_to::<AssetPushBegin>(AssetPushBeginAck {
            resume_from_offset: resume_from_offset as u32,
          })
          .await;
      }
      Err(err) => {
        PENDING_PUSHES.drop_pending(&id);
        self
          .handle
          .respond_err::<AssetPushBegin>(AssetPushBeginRejected {
            reason: format!("transfer begin failed: {err}"),
          })
          .await;
      }
    }
    Ok(())
  }

  async fn handle_push_chunk(self, chunk: AssetPushChunk) -> HandlerResult {
    let AssetPushChunk {
      id,
      offset,
      bytes,
      last,
    } = chunk;
    tracing::trace!(
      id = %id,
      offset,
      len = bytes.len(),
      last,
      "({:?}) AssetPushChunk from gateway",
      &self.handle.address,
    );

    let outcome = self
      .handle
      .state
      .transfers
      .accept_chunk(id.clone(), offset as u64, Bytes::from(bytes), last)
      .await;

    match outcome {
      Ok(ChunkOutcome::Continue { .. }) => {}
      Ok(ChunkOutcome::Completed { path, .. }) => {
        let Some(pending) = PENDING_PUSHES.take_pending(&id) else {
          tracing::warn!(id = %id, "AssetPushChunk completed but no PendingPush state; dropping partial");
          let _ = tokio::fs::remove_file(&path).await;
          return Ok(());
        };
        if let Err(err) = self
          .handle
          .state
          .assets
          .insert_from_path(id.clone(), path, pending.mime, pending.retention)
          .await
        {
          tracing::error!(?err, id = %id, "asset cache insert_from_path failed");
        }
      }
      Err(err) => {
        tracing::warn!(?err, id = %id, "AssetPushChunk: chunk rejected by transfer");
        PENDING_PUSHES.drop_pending(&id);
        let _ = self.handle.state.transfers.abandon(id).await;
      }
    }
    Ok(())
  }

  async fn handle_push_abandon(self, req: AssetPushAbandon) -> HandlerResult {
    tracing::info!(id = %req.id, "({:?}) AssetPushAbandon from gateway", &self.handle.address);
    PENDING_PUSHES.drop_pending(&req.id);
    if let Err(err) = self.handle.state.transfers.abandon(req.id).await {
      tracing::warn!(?err, "transfer abandon failed");
    }
    Ok(())
  }
}

/// Per-id state the AssetHandler needs to hold between `PushBegin` and
/// the final `PushChunk(last:true)` - the retention + mime that will
/// be passed to `AssetCache::insert_from_path` once the bytes finish
/// landing. ChunkedTransfer is an asset-agnostic primitive so it
/// doesn't carry these; we keep them here in a process-wide registry.
struct PendingPush {
  mime: Option<String>,
  retention: AssetRetention,
}

static PENDING_PUSHES: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<String, PendingPush>>> =
  std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

trait PendingPushRegistryExt {
  fn with_pending(&self, id: String, pending: PendingPush);
  fn take_pending(&self, id: &str) -> Option<PendingPush>;
  fn drop_pending(&self, id: &str);
}

impl PendingPushRegistryExt for std::sync::Mutex<std::collections::HashMap<String, PendingPush>> {
  fn with_pending(&self, id: String, pending: PendingPush) {
    self.lock().expect("PENDING_PUSHES poisoned").insert(id, pending);
  }
  fn take_pending(&self, id: &str) -> Option<PendingPush> {
    self.lock().expect("PENDING_PUSHES poisoned").remove(id)
  }
  fn drop_pending(&self, id: &str) {
    self.lock().expect("PENDING_PUSHES poisoned").remove(id);
  }
}
