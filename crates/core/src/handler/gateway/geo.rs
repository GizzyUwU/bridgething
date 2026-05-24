use libbridgething::{Position, client::BridgeToClientGeoMsgEvent, gateway::GatewayToBridgeGeoMsgEventDispatch};

use super::{HandlerResult, MsgHandle};

pub struct GeoHandler {
  handle: MsgHandle,
}

impl GeoHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }
}

impl GatewayToBridgeGeoMsgEventDispatch for GeoHandler {
  type Output = HandlerResult;

  async fn position(&self, params: Position) -> HandlerResult {
    let owners = self.handle.state.geo_watchers.owners();
    if owners.is_empty() {
      tracing::trace!("geo position arrived with no watchers; dropping");
      return Ok(());
    }
    for owner in owners {
      let event = BridgeToClientGeoMsgEvent::Position(params);
      if let Err(err) = self.handle.state.bus.send_event(owner, event).await {
        tracing::warn!(?err, %owner, "failed to forward geo position to webapp");
      }
    }
    Ok(())
  }
}
