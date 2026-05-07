use libbridgething::gateway::{
  AssetClear, AssetPush, AssetPushAbandon, AssetPushBegin, AssetPushBeginAck, AssetPushBeginRejected, AssetPushChunk,
  GatewayToBridgeAssetMsgCommand, GatewayToBridgeAssetMsgEvent, GatewayToBridgeAssetMsgRequest,
};
use tokio_util::bytes::Bytes;

use super::{HandlerResult, MsgHandle};

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
      GatewayToBridgeAssetMsgEvent::Push(p) => self.handle_push(p).await,
      GatewayToBridgeAssetMsgEvent::Clear(c) => self.handle_clear(c).await,
      GatewayToBridgeAssetMsgEvent::PushChunk(chunk) => self.handle_push_chunk(chunk).await,
    }
    Ok(())
  }

  pub async fn handle_command(self, cmd: GatewayToBridgeAssetMsgCommand) -> HandlerResult {
    match cmd {
      GatewayToBridgeAssetMsgCommand::PushAbandon(req) => self.handle_push_abandon(req).await,
    }
    Ok(())
  }

  pub async fn handle_request(self, req: GatewayToBridgeAssetMsgRequest) -> HandlerResult {
    match req {
      GatewayToBridgeAssetMsgRequest::PushBegin(begin) => self.handle_push_begin(begin).await,
    }
    Ok(())
  }

  async fn handle_push(self, push: AssetPush) {
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
    self
      .handle
      .state
      .ingest
      .push(id, Bytes::from(bytes), mime, retention)
      .await;
  }

  async fn handle_clear(self, clear: AssetClear) {
    tracing::debug!(id = %clear.id, "({:?}) asset clear from gateway", &self.handle.address);
    self.handle.state.ingest.clear(clear.id).await;
  }

  async fn handle_push_chunk(self, chunk: AssetPushChunk) {
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
    self
      .handle
      .state
      .ingest
      .chunk(id, offset as u64, Bytes::from(bytes), last)
      .await;
  }

  async fn handle_push_abandon(self, req: AssetPushAbandon) {
    tracing::info!(id = %req.id, "({:?}) AssetPushAbandon from gateway", &self.handle.address);
    self.handle.state.ingest.abandon(req.id).await;
  }

  async fn handle_push_begin(self, begin: AssetPushBegin) {
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

    match self
      .handle
      .state
      .ingest
      .begin(id, expected_size as u64, expected_sha256, mime, retention)
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
        self
          .handle
          .respond_err::<AssetPushBegin>(AssetPushBeginRejected {
            reason: format!("transfer begin failed: {err}"),
          })
          .await;
      }
    }
  }
}
