use libbridgething::gateway::GatewayToBridgeAuthorityMsgEvent;

use super::{HandlerResult, MsgHandle};

#[derive(Debug)]
pub struct AuthorityHandler {
  handle: MsgHandle,
}

impl AuthorityHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(&mut self, msg: GatewayToBridgeAuthorityMsgEvent) -> HandlerResult {
    match msg {
      GatewayToBridgeAuthorityMsgEvent::Claim(claim) => {
        tracing::debug!(scope = ?claim.scope, "({:?}) companion claims authority", &self.handle.address);
        if let Err(err) = self.handle.state.capabilities.claim_authority(claim.scope).await {
          tracing::warn!(?err, "failed to publish authority claim");
        }
      }
      GatewayToBridgeAuthorityMsgEvent::Release(release) => {
        tracing::debug!(scope = ?release.scope, "({:?}) companion releases authority", &self.handle.address);
        if let Err(err) = self.handle.state.capabilities.release_authority(release.scope).await {
          tracing::warn!(?err, "failed to publish authority release");
        }
      }
    }
    Ok(())
  }
}
