use libbridgething::{
  AssetRetention,
  gateway::{AssetClear, AssetPush, GatewayToBridgeAssetMsgEvent},
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

  pub async fn handle(&mut self, msg: GatewayToBridgeAssetMsgEvent) -> HandlerResult {
    match msg {
      GatewayToBridgeAssetMsgEvent::Push(push) => self.handle_push(push).await,
      GatewayToBridgeAssetMsgEvent::Clear(clear) => self.handle_clear(clear).await,
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
      "({:?}) asset push from gateway",
      &self.handle.address,
    );

    if let Err(err) = self
      .handle
      .state
      .assets
      .insert(id, Bytes::from(bytes), mime, normalize_retention(retention))
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
}

/// Apply any defaults the wire shape might leave open. Today this is
/// a no-op (every variant is fully specified) but it's the right place
/// for forward-compat normalisation.
fn normalize_retention(retention: AssetRetention) -> AssetRetention {
  retention
}
