use libbridgething::gateway::{
  AssetClear, AssetPush, AssetPushAbandon, AssetPushBegin, AssetPushBeginAck, AssetPushBeginRejected, AssetPushChunk,
  GatewayToBridgeAssetMsgCommandDispatch, GatewayToBridgeAssetMsgEventDispatch, GatewayToBridgeAssetMsgRequestDispatch,
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
}

impl GatewayToBridgeAssetMsgEventDispatch for AssetHandler {
  type Output = HandlerResult;

  async fn push(&self, params: AssetPush) -> HandlerResult {
    let AssetPush {
      id,
      bytes,
      mime,
      retention,
    } = params;
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
    Ok(())
  }

  async fn clear(&self, params: AssetClear) -> HandlerResult {
    tracing::debug!(id = %params.id, "({:?}) asset clear from gateway", &self.handle.address);
    self.handle.state.ingest.clear(params.id).await;
    Ok(())
  }

  async fn push_chunk(&self, params: AssetPushChunk) -> HandlerResult {
    let AssetPushChunk {
      id,
      offset,
      bytes,
      last,
    } = params;
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
    Ok(())
  }
}

impl GatewayToBridgeAssetMsgCommandDispatch for AssetHandler {
  type Output = HandlerResult;

  async fn push_abandon(&self, params: AssetPushAbandon) -> HandlerResult {
    tracing::info!(id = %params.id, "({:?}) AssetPushAbandon from gateway", &self.handle.address);
    self.handle.state.ingest.abandon(params.id).await;
    Ok(())
  }
}

impl GatewayToBridgeAssetMsgRequestDispatch for AssetHandler {
  type Output = HandlerResult;

  async fn push_begin(&self, params: AssetPushBegin) -> HandlerResult {
    let AssetPushBegin {
      id,
      expected_size,
      expected_sha256,
      mime,
      retention,
    } = params;
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
    Ok(())
  }
}
