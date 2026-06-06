use libbridgething::gateway::{AssetClear, GatewayToBridgeAssetMsgEventDispatch};

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

  async fn clear(&self, params: AssetClear) -> HandlerResult {
    tracing::debug!(id = %params.id, "({:?}) asset clear from gateway", &self.handle.address);
    if let Err(err) = self.handle.state.assets.clear(&params.id).await {
      tracing::warn!(?err, id = %params.id, "asset clear failed");
    }
    Ok(())
  }
}
