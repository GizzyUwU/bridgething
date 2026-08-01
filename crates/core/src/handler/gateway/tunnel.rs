use libbridgething::{TunnelAck, TunnelClosed, TunnelData, gateway::GatewayToBridgeTunnelMsgEventDispatch};

use super::{HandlerResult, MsgHandle};
use crate::state::TunnelInbound;

pub struct TunnelHandler {
  handle: MsgHandle,
}

impl TunnelHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }
}

impl GatewayToBridgeTunnelMsgEventDispatch for TunnelHandler {
  type Output = HandlerResult;

  async fn data(&self, params: TunnelData) -> HandlerResult {
    if let Some(tx) = self.handle.state.tunnel_routes.lookup(params.tunnel_id) {
      let _ = tx.send(TunnelInbound::Data(params.bytes)).await;
    } else {
      tracing::trace!(tunnel_id = %params.tunnel_id, "tunnel data for unknown tunnel; dropping");
    }
    Ok(())
  }

  async fn ack(&self, params: TunnelAck) -> HandlerResult {
    self
      .handle
      .state
      .tunnel_routes
      .note_ack(params.tunnel_id, params.consumed);
    Ok(())
  }

  async fn closed(&self, params: TunnelClosed) -> HandlerResult {
    if let Some(tx) = self.handle.state.tunnel_routes.drop_id(params.tunnel_id) {
      let _ = tx.send(TunnelInbound::Closed(params.reason)).await;
    } else {
      tracing::trace!(tunnel_id = %params.tunnel_id, "tunnel closed for unknown tunnel; dropping");
    }
    Ok(())
  }
}
