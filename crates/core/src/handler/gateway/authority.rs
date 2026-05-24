use libbridgething::gateway::{AuthorityClaim, AuthorityRelease, GatewayToBridgeAuthorityMsgEventDispatch};

use super::{HandlerResult, MsgHandle};

#[derive(Debug)]
pub struct AuthorityHandler {
  handle: MsgHandle,
}

impl AuthorityHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }
}

impl GatewayToBridgeAuthorityMsgEventDispatch for AuthorityHandler {
  type Output = HandlerResult;

  async fn claim(&self, params: AuthorityClaim) -> HandlerResult {
    tracing::debug!(scope = ?params.scope, "({:?}) companion claims authority", &self.handle.address);
    if let Err(err) = self.handle.state.capabilities.claim_authority(params.scope).await {
      tracing::warn!(?err, "failed to publish authority claim");
    }
    Ok(())
  }

  async fn release(&self, params: AuthorityRelease) -> HandlerResult {
    tracing::debug!(scope = ?params.scope, "({:?}) companion releases authority", &self.handle.address);
    if let Err(err) = self.handle.state.capabilities.release_authority(params.scope).await {
      tracing::warn!(?err, "failed to publish authority release");
    }
    Ok(())
  }
}
