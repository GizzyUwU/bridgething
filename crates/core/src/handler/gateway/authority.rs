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
        self.handle.state.authority.claim(claim.scope);
      }
      GatewayToBridgeAuthorityMsgEvent::Release(release) => {
        tracing::debug!(scope = ?release.scope, "({:?}) companion releases authority", &self.handle.address);
        self.handle.state.authority.release(release.scope);
      }
    }
    Ok(())
  }
}
