use libbridgething::gateway::GatewayToBridgeTunnelMsgEvent;

use super::{HandlerResult, MsgHandle};
use crate::state::TunnelInbound;

pub struct TunnelHandler {
  handle: MsgHandle,
}

impl TunnelHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: GatewayToBridgeTunnelMsgEvent) -> HandlerResult {
    match msg {
      GatewayToBridgeTunnelMsgEvent::Data(data) => {
        if let Some(tx) = self.handle.state.tunnel_routes.lookup(data.tunnel_id) {
          let _ = tx.send(TunnelInbound::Data(data.bytes)).await;
        } else {
          tracing::trace!(tunnel_id = %data.tunnel_id, "tunnel data for unknown tunnel; dropping");
        }
      }
      GatewayToBridgeTunnelMsgEvent::Closed(closed) => {
        if let Some(tx) = self.handle.state.tunnel_routes.drop_id(closed.tunnel_id) {
          let _ = tx.send(TunnelInbound::Closed(closed.reason)).await;
        } else {
          tracing::trace!(tunnel_id = %closed.tunnel_id, "tunnel closed for unknown tunnel; dropping");
        }
      }
    }
    Ok(())
  }
}
