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
    tracing::debug!(scope = ?params.scope, app_bundle = ?params.app_bundle, "({:?}) companion claims authority", &self.handle.address);
    let Some(addr) = self.handle.address else {
      tracing::warn!(scope = ?params.scope, "authority claim from an unaddressed gateway; ignoring");
      return Ok(());
    };
    if let Err(err) = self
      .handle
      .state
      .capabilities
      .claim_authority(addr, params.scope, params.app_bundle)
      .await
    {
      tracing::warn!(?err, "failed to publish authority claim");
    }
    self.handle.state.player.note_authority_changed().await?;
    Ok(())
  }

  async fn release(&self, params: AuthorityRelease) -> HandlerResult {
    tracing::debug!(scope = ?params.scope, "({:?}) companion releases authority", &self.handle.address);
    let Some(addr) = self.handle.address else {
      tracing::warn!(scope = ?params.scope, "authority release from an unaddressed gateway; ignoring");
      return Ok(());
    };
    if let Err(err) = self
      .handle
      .state
      .capabilities
      .release_authority(addr, params.scope)
      .await
    {
      tracing::warn!(?err, "failed to publish authority release");
    }
    self.handle.state.player.note_authority_changed().await?;
    Ok(())
  }
}
